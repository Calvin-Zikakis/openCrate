//! The player: one engine thread decodes ahead into a short buffer, and the sound device drains
//! it. Commands go to the engine; the status is read at any time.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::source::Source;
use crate::Error;

/// How far ahead the engine decodes: half a second.
const AHEAD_SECONDS_DIVISOR: usize = 2;

/// What the sound device and the engine share.
pub struct Shared {
    buffer: Mutex<VecDeque<f32>>,
    playing: AtomicBool,
    frames_played: AtomicU64,
    volume: AtomicU32,
}

impl Shared {
    fn new() -> Self {
        Self {
            buffer: Mutex::new(VecDeque::new()),
            playing: AtomicBool::new(false),
            frames_played: AtomicU64::new(0),
            volume: AtomicU32::new(1.0_f32.to_bits()),
        }
    }

    fn buffer(&self) -> std::sync::MutexGuard<'_, VecDeque<f32>> {
        self.buffer.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Fill one device block (`channels` samples a frame) from the buffer: silence when paused
    /// or when the buffer runs dry. Called on the device's thread.
    pub fn fill(&self, out: &mut [f32], channels: usize) {
        out.fill(0.0);
        if channels == 0 || !self.playing.load(Ordering::Acquire) {
            return;
        }
        let volume = f32::from_bits(self.volume.load(Ordering::Relaxed));
        let mut buffer = self.buffer();
        let mut played = 0_u64;
        for frame in out.chunks_exact_mut(channels) {
            let (Some(left), Some(right)) = (buffer.pop_front(), buffer.pop_front()) else {
                break;
            };
            if channels == 1 {
                frame[0] = f32::midpoint(left, right) * volume;
            } else {
                frame[0] = left * volume;
                frame[1] = right * volume;
            }
            played += 1;
        }
        self.frames_played.fetch_add(played, Ordering::Release);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// What the caller named the loaded track, such as its id.
    pub key: Option<String>,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    /// The track played to its end.
    pub ended: bool,
    pub error: Option<String>,
}

enum Command {
    Load {
        path: PathBuf,
        key: String,
        start_ms: u64,
        play: bool,
    },
    Play,
    Pause,
    Seek(u64),
    Stop,
}

/// Opens the sound device on the engine thread; returns what keeps it open and its rate.
type Device = Box<dyn FnOnce(Arc<Shared>) -> Result<(Box<dyn std::any::Any>, u32), Error> + Send>;

pub struct Player {
    commands: Mutex<Sender<Command>>,
    shared: Arc<Shared>,
    status: Arc<Mutex<Status>>,
}

impl Player {
    /// A player on the default sound output. The device is opened when the first track loads.
    #[must_use]
    pub fn start() -> Self {
        Self::with_device(Box::new(|shared| {
            let (stream, rate) = crate::output::open(shared)?;
            Ok((Box::new(stream), rate))
        }))
    }

    fn with_device(device: Device) -> Self {
        let (sender, receiver) = channel();
        let shared = Arc::new(Shared::new());
        let status = Arc::new(Mutex::new(Status::default()));
        let (engine_shared, engine_status) = (Arc::clone(&shared), Arc::clone(&status));
        std::thread::Builder::new()
            .name("opencrate-player".into())
            .spawn(move || {
                // Built on its own thread: the device stream it will hold may not move threads.
                let engine = Engine {
                    shared: engine_shared,
                    status: engine_status,
                    device: Some(device),
                    keep: None,
                    rate: 0,
                    source: None,
                    offset_ms: 0,
                    drained: false,
                };
                engine.run(&receiver);
            })
            .ok();
        Self {
            commands: Mutex::new(sender),
            shared,
            status,
        }
    }

    fn send(&self, command: Command) {
        let _ = self
            .commands
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(command);
    }

    /// Load a file (read only) and optionally start playing from `start_ms`.
    pub fn load(&self, path: PathBuf, key: String, start_ms: u64, play: bool) {
        self.send(Command::Load {
            path,
            key,
            start_ms,
            play,
        });
    }

    pub fn play(&self) {
        self.send(Command::Play);
    }

    pub fn pause(&self) {
        self.send(Command::Pause);
    }

    pub fn seek(&self, ms: u64) {
        self.send(Command::Seek(ms));
    }

    pub fn stop(&self) {
        self.send(Command::Stop);
    }

    /// 0 is silent, 1 is full volume.
    pub fn set_volume(&self, volume: f32) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.shared
            .volume
            .store(volume.to_bits(), Ordering::Relaxed);
    }

    #[must_use]
    pub fn status(&self) -> Status {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

struct Engine {
    shared: Arc<Shared>,
    status: Arc<Mutex<Status>>,
    device: Option<Device>,
    /// Keeps the device stream alive.
    keep: Option<Box<dyn std::any::Any>>,
    rate: u32,
    source: Option<Source>,
    offset_ms: u64,
    /// The source has no more audio.
    drained: bool,
}

impl Engine {
    fn run(mut self, receiver: &std::sync::mpsc::Receiver<Command>) {
        loop {
            let busy = self.source.is_some() && !self.drained;
            let wait = Duration::from_millis(if busy { 5 } else { 40 });
            match receiver.recv_timeout(wait) {
                Ok(command) => self.handle(command),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.decode_ahead();
            self.report();
        }
    }

    fn set_error(&self, message: String) {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .error = Some(message);
    }

    fn ensure_device(&mut self) -> bool {
        if self.keep.is_some() {
            return true;
        }
        let Some(open) = self.device.take() else {
            return false;
        };
        match open(Arc::clone(&self.shared)) {
            Ok((keep, rate)) => {
                self.keep = Some(keep);
                self.rate = rate;
                true
            }
            Err(error) => {
                self.set_error(error.to_string());
                false
            }
        }
    }

    fn restart_at(&mut self, ms: u64) {
        let Some(source) = self.source.as_mut() else {
            return;
        };
        match source.seek(ms) {
            Ok(actual) => {
                self.shared.buffer().clear();
                self.shared.frames_played.store(0, Ordering::Release);
                self.offset_ms = actual;
                self.drained = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Load {
                path,
                key,
                start_ms,
                play,
            } => {
                self.shared.playing.store(false, Ordering::Release);
                self.shared.buffer().clear();
                self.shared.frames_played.store(0, Ordering::Release);
                self.source = None;
                {
                    let mut status = self.status.lock().unwrap_or_else(PoisonError::into_inner);
                    *status = Status {
                        key: Some(key),
                        ..Status::default()
                    };
                }
                if !self.ensure_device() {
                    return;
                }
                match Source::open(&path, self.rate) {
                    Ok(source) => {
                        self.status
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .duration_ms = source.duration_ms;
                        self.source = Some(source);
                        self.offset_ms = 0;
                        self.drained = false;
                        if start_ms > 0 {
                            self.restart_at(start_ms);
                        }
                        self.shared.playing.store(play, Ordering::Release);
                    }
                    Err(error) => self.set_error(error.to_string()),
                }
            }
            Command::Play => {
                if self.source.is_some() {
                    if self.drained && self.shared.buffer().is_empty() {
                        self.restart_at(0);
                    }
                    self.shared.playing.store(true, Ordering::Release);
                }
            }
            Command::Pause => self.shared.playing.store(false, Ordering::Release),
            Command::Seek(ms) => self.restart_at(ms),
            Command::Stop => {
                self.shared.playing.store(false, Ordering::Release);
                self.shared.buffer().clear();
                self.source = None;
                *self.status.lock().unwrap_or_else(PoisonError::into_inner) = Status::default();
            }
        }
    }

    fn decode_ahead(&mut self) {
        let Some(source) = self.source.as_mut() else {
            return;
        };
        let target = self.rate as usize * 2 / AHEAD_SECONDS_DIVISOR;
        while !self.drained && self.shared.buffer().len() < target {
            match source.next_block() {
                Ok(Some(block)) => self.shared.buffer().extend(block),
                Ok(None) => self.drained = true,
                Err(error) => {
                    self.drained = true;
                    let message = error.to_string();
                    self.status
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .error = Some(message);
                }
            }
        }
    }

    fn report(&self) {
        let played = self.shared.frames_played.load(Ordering::Acquire);
        let empty = self.shared.buffer().is_empty();
        let ended = self.source.is_some() && self.drained && empty;
        if ended {
            self.shared.playing.store(false, Ordering::Release);
        }
        let mut status = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        if self.rate > 0 {
            status.position_ms = self.offset_ms + played * 1000 / u64::from(self.rate);
        }
        status.playing = self.shared.playing.load(Ordering::Acquire);
        status.ended = ended;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Instant;

    use super::*;

    const RATE: u32 = 44_100;

    /// A player with no sound device; the test drains it as a device would.
    fn headless() -> Player {
        Player::with_device(Box::new(|_| Ok((Box::new(()), RATE))))
    }

    fn wait_for(player: &Player, what: impl Fn(&Status) -> bool) -> Status {
        let start = Instant::now();
        loop {
            let status = player.status();
            if what(&status) {
                return status;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "timed out at {status:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Pull `ms` of audio as the device would.
    fn drain(player: &Player, ms: u64) {
        let frames = RATE as usize * usize::try_from(ms).unwrap() / 1000;
        let mut done = 0;
        while done < frames {
            let block = (frames - done).min(512);
            let mut out = vec![0.0; block * 2];
            player.shared.fill(&mut out, 2);
            done += block;
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn track(seconds: u32) -> (tempfile::TempDir, PathBuf) {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.wav");
        fs::write(&path, oc_tag::fixtures::wav(seconds)).unwrap();
        (folder, path)
    }

    #[test]
    fn a_track_plays_pauses_seeks_and_ends() {
        let (_folder, path) = track(2);
        let player = headless();
        player.load(path, "7".into(), 0, true);
        let status = wait_for(&player, |s| s.playing && s.duration_ms == Some(2_000));
        assert_eq!(status.key.as_deref(), Some("7"));
        drain(&player, 500);
        let at = wait_for(&player, |s| s.position_ms >= 500).position_ms;
        assert!(at <= 520, "{at}");
        player.pause();
        wait_for(&player, |s| !s.playing);
        drain(&player, 300);
        let paused = player.status().position_ms;
        assert!(
            paused.abs_diff(at) <= 20,
            "paused stays put: {at} -> {paused}"
        );
        player.seek(1_800);
        player.play();
        wait_for(&player, |s| s.playing && s.position_ms >= 1_700);
        drain(&player, 400);
        let end = wait_for(&player, |s| s.ended);
        assert!(!end.playing && end.position_ms >= 1_950, "{end:?}");
        player.play();
        let again = wait_for(&player, |s| s.playing && !s.ended);
        assert!(
            again.position_ms < 100,
            "play after the end starts over: {again:?}"
        );
        player.stop();
        assert_eq!(wait_for(&player, |s| s.key.is_none()), Status::default());
    }

    #[test]
    fn a_track_can_start_partway_and_a_bad_file_says_so() {
        let (folder, path) = track(3);
        let player = headless();
        player.load(path, "1".into(), 2_000, false);
        let status = wait_for(&player, |s| s.position_ms >= 1_900);
        assert!(!status.playing);
        let bad = folder.path().join("bad.mp3");
        fs::write(&bad, b"nope").unwrap();
        player.load(bad, "2".into(), 0, true);
        let failed = wait_for(&player, |s| s.error.is_some());
        assert_eq!(failed.key.as_deref(), Some("2"));
        assert!(!failed.playing);
    }

    #[test]
    fn volume_and_channels_shape_what_the_device_gets() {
        let shared = Shared::new();
        shared.buffer().extend([0.5, -0.5, 0.25, 0.25]);
        let mut out = [9.0; 4];
        shared.fill(&mut out, 2);
        assert_eq!(out, [0.0; 4], "paused is silent and keeps the buffer");
        shared.playing.store(true, Ordering::Release);
        shared.volume.store(0.5_f32.to_bits(), Ordering::Relaxed);
        let mut mono = [9.0; 3];
        shared.fill(&mut mono, 1);
        assert_eq!(
            mono,
            [0.0, 0.125, 0.0],
            "mono mixes, volume halves, a dry buffer is silent"
        );
        assert_eq!(shared.frames_played.load(Ordering::Acquire), 2);
        let player = headless();
        player.set_volume(f32::NAN);
        assert!(
            (f32::from_bits(player.shared.volume.load(Ordering::Relaxed)) - 1.0).abs()
                < f32::EPSILON
        );
        player.set_volume(3.0);
        assert!(
            (f32::from_bits(player.shared.volume.load(Ordering::Relaxed)) - 1.0).abs()
                < f32::EPSILON
        );
    }

    /// Plays a silent file on the real default output for a moment. Run by hand:
    /// `cargo test -p oc-player live_device -- --ignored`.
    #[test]
    #[ignore = "uses the sound device"]
    fn live_device() {
        let (_folder, path) = track(2);
        let player = Player::start();
        player.load(path, "live".into(), 0, true);
        let status = wait_for(&player, |s| s.position_ms >= 300 || s.error.is_some());
        assert_eq!(status.error, None);
        player.stop();
    }
}
