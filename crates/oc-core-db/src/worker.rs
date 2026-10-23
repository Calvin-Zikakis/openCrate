//! The application's one writer thread and its pool of cached readers. SQLite
//! allows one writer, so every write goes through [`Writer`] in order; readers
//! never wait for it (WAL), and each keeps its own cached id lists.

use std::ops::{Deref, DerefMut};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Sender};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

use crate::library::LibraryReader;
use crate::{Database, Error, Result};

type Job = Box<dyn FnOnce(&mut Database) + Send>;

enum Message {
    Job(Job),
    /// Close the database cleanly and stop.
    Shutdown,
}

/// Owns the [`Database`] on its own thread. Only [`Writer::shutdown`] closes it
/// cleanly; dropping the handle behaves like dropping the database and leaves the
/// dirty marker for the next open to check.
pub struct Writer {
    jobs: Option<Sender<Message>>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl Writer {
    #[must_use]
    pub fn spawn(mut database: Database) -> Self {
        let (jobs, receiver) = mpsc::channel::<Message>();
        let thread = thread::spawn(move || {
            while let Ok(message) = receiver.recv() {
                match message {
                    Message::Job(job) => job(&mut database),
                    Message::Shutdown => return database.close(),
                }
            }
            // The handle was dropped without `shutdown`: leave the dirty marker.
            Ok(())
        });
        Self {
            jobs: Some(jobs),
            thread: Some(thread),
        }
    }

    /// Run `work` on the writer thread and wait for its result. A panic in `work`
    /// is reported as an error and the worker keeps serving.
    pub fn run<R, F>(&self, work: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Database) -> R + Send + 'static,
    {
        let (reply, result) = mpsc::channel();
        let job: Job = Box::new(move |database| {
            let outcome = catch_unwind(AssertUnwindSafe(|| work(database)));
            // The caller may have stopped waiting; nothing more to do then.
            let _ = reply.send(outcome.map_err(|_| Error::WorkerPanicked));
        });
        self.jobs
            .as_ref()
            .ok_or(Error::WorkerStopped)?
            .send(Message::Job(job))
            .map_err(|_| Error::WorkerStopped)?;
        result.recv().map_err(|_| Error::WorkerStopped)?
    }

    /// Finish queued work, then close the database cleanly.
    pub fn shutdown(mut self) -> Result<()> {
        if let Some(jobs) = self.jobs.take() {
            jobs.send(Message::Shutdown)
                .map_err(|_| Error::WorkerStopped)?;
        }
        match self.thread.take() {
            Some(thread) => thread.join().map_err(|_| Error::WorkerPanicked)?,
            None => Ok(()),
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.jobs = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A fixed set of cached readers. [`ReaderPool::lease`] blocks while all are in use.
pub struct ReaderPool {
    idle: Mutex<Vec<LibraryReader>>,
    returned: Condvar,
}

impl ReaderPool {
    /// `size` must be at least one.
    pub fn new(database: &Database, size: usize) -> Result<Self> {
        let readers = (0..size.max(1))
            .map(|_| LibraryReader::new(database.reader()?))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            idle: Mutex::new(readers),
            returned: Condvar::new(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Vec<LibraryReader>> {
        self.idle.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn lease(&self) -> ReaderLease<'_> {
        let mut idle = self.lock();
        loop {
            if let Some(reader) = idle.pop() {
                return ReaderLease {
                    pool: self,
                    reader: Some(reader),
                };
            }
            idle = self
                .returned
                .wait(idle)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// A reader on loan. Returned to the pool when dropped, including after a panic.
pub struct ReaderLease<'a> {
    pool: &'a ReaderPool,
    reader: Option<LibraryReader>,
}

impl Deref for ReaderLease<'_> {
    type Target = LibraryReader;

    fn deref(&self) -> &LibraryReader {
        // Present from construction until drop.
        self.reader.as_ref().unwrap_or_else(|| unreachable!())
    }
}

impl DerefMut for ReaderLease<'_> {
    fn deref_mut(&mut self) -> &mut LibraryReader {
        self.reader.as_mut().unwrap_or_else(|| unreachable!())
    }
}

impl Drop for ReaderLease<'_> {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take() {
            self.pool.lock().push(reader);
            self.pool.returned.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use tempfile::tempdir;

    use super::*;
    use crate::library::{LibraryQuery, View};
    use crate::synthetic::seed;
    use crate::OpenStatus;

    #[test]
    fn the_writer_runs_jobs_in_order_and_shutdown_closes_cleanly() {
        let directory = tempdir().unwrap();
        let writer = Writer::spawn(Database::open(directory.path()).unwrap());
        for value in 0..5 {
            let inserted = writer
                .run(move |db| {
                    db.connection
                        .execute(
                            "INSERT INTO artist (name, sort_key) VALUES (?1, ?1)",
                            [format!("artist {value}")],
                        )
                        .unwrap();
                    db.connection.last_insert_rowid()
                })
                .unwrap();
            assert_eq!(inserted, value + 1);
        }
        writer.shutdown().unwrap();
        let database = Database::open(directory.path()).unwrap();
        assert_eq!(database.status(), OpenStatus::Clean);
        database.close().unwrap();
    }

    #[test]
    fn dropping_the_writer_leaves_the_dirty_marker() {
        let directory = tempdir().unwrap();
        drop(Writer::spawn(Database::open(directory.path()).unwrap()));
        let database = Database::open(directory.path()).unwrap();
        assert_eq!(database.status(), OpenStatus::CheckedAfterUncleanShutdown);
    }

    #[test]
    fn a_panicking_job_is_an_error_and_the_worker_survives() {
        let directory = tempdir().unwrap();
        let writer = Writer::spawn(Database::open(directory.path()).unwrap());
        let failed: Result<()> = writer.run(|_| panic!("job failed"));
        assert!(matches!(failed, Err(Error::WorkerPanicked)));
        assert_eq!(writer.run(|_| 7).unwrap(), 7);
        writer.shutdown().unwrap();
    }

    #[test]
    fn many_threads_share_one_writer() {
        let directory = tempdir().unwrap();
        let writer = Arc::new(Writer::spawn(Database::open(directory.path()).unwrap()));
        let handles: Vec<_> = (0..8)
            .map(|thread| {
                let writer = Arc::clone(&writer);
                thread::spawn(move || {
                    for n in 0..25 {
                        writer
                            .run(move |db| {
                                db.connection
                                    .execute(
                                        "INSERT INTO artist (name, sort_key) VALUES (?1, ?1)",
                                        [format!("{thread}-{n}")],
                                    )
                                    .unwrap();
                            })
                            .unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let count = writer
            .run(|db| {
                db.connection
                    .query_row("SELECT COUNT(*) FROM artist", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap()
            })
            .unwrap();
        assert_eq!(count, 200);
        Arc::into_inner(writer).unwrap().shutdown().unwrap();
    }

    #[test]
    fn a_pool_of_one_makes_the_second_borrower_wait_for_the_first() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let pool = Arc::new(ReaderPool::new(&database, 1).unwrap());
        let events = Arc::new(Mutex::new(Vec::new()));
        let first = pool.lease();
        let waiter = {
            let (pool, events) = (Arc::clone(&pool), Arc::clone(&events));
            thread::spawn(move || {
                let _second = pool.lease();
                events.lock().unwrap().push("second leased");
            })
        };
        events.lock().unwrap().push("first releasing");
        drop(first);
        waiter.join().unwrap();
        assert_eq!(
            *events.lock().unwrap(),
            ["first releasing", "second leased"]
        );
    }

    #[test]
    fn a_reader_is_returned_even_if_the_borrower_panics() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let pool = Arc::new(ReaderPool::new(&database, 1).unwrap());
        let crashed = {
            let pool = Arc::clone(&pool);
            thread::spawn(move || {
                let _lease = pool.lease();
                panic!("borrower failed");
            })
            .join()
        };
        assert!(crashed.is_err());
        let _again = pool.lease();
    }

    #[test]
    fn readers_see_each_commit_while_the_writer_keeps_going() {
        let directory = tempdir().unwrap();
        let mut database = Database::open(directory.path()).unwrap();
        seed(&mut database, 100).unwrap();
        let pool = Arc::new(ReaderPool::new(&database, 3).unwrap());
        let writer = Arc::new(Writer::spawn(database));
        let finished = Arc::new(AtomicBool::new(false));
        let readers: Vec<_> = (0..3)
            .map(|_| {
                let (pool, finished) = (Arc::clone(&pool), Arc::clone(&finished));
                thread::spawn(move || {
                    let query = LibraryQuery::new(View::Tracks);
                    let mut last = 0;
                    while !finished.load(Ordering::Relaxed) {
                        let page = pool.lease().read_page(&query, 0, 20).unwrap();
                        assert!(page.total >= last, "a total never goes backwards");
                        assert!(page.total >= 100 && page.total <= 150);
                        assert_eq!(
                            page.items.len(),
                            20,
                            "the window is always complete even as rows arrive"
                        );
                        last = page.total;
                    }
                    last
                })
            })
            .collect();
        for n in 0..50 {
            writer
                .run(move |db| {
                    db.connection
                        .execute(
                            "INSERT INTO track (release_id, artist_id, title, title_key, \
                             artist_key, added_ms) VALUES (1, 1, 'New', 'new', 'x', ?1)",
                            [10_000_000_000_000_i64 + n],
                        )
                        .unwrap();
                })
                .unwrap();
        }
        finished.store(true, Ordering::Relaxed);
        for reader in readers {
            reader.join().unwrap();
        }
        let page = pool
            .lease()
            .read_page(&LibraryQuery::new(View::Tracks), 0, 1)
            .unwrap();
        assert_eq!(page.total, 150, "the final state is visible");
        Arc::into_inner(writer).unwrap().shutdown().unwrap();
    }
}
