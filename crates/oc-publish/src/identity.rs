//! What "the same file" means: size and a SHA-256 of the content.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub size: u64,
    pub sha256: [u8; 32],
}

/// Read the whole file once. `None` if it does not exist; any other failure is an error.
pub fn identity_of(path: &Path) -> io::Result<Option<Identity>> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1 << 20];
    let mut size = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok(Some(Identity {
        size,
        sha256: hasher.finalize().into(),
    }))
}
