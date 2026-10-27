//! Starting ffmpeg. On Windows a desktop app that starts a console program gets a console window
//! for it unless it asks for none, which looked like a terminal opening per file during an import.

use std::ffi::OsStr;
use std::process::Command;

/// A command for `program` that opens no window of its own.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut, reason = "only Windows changes it")]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// `CREATE_NO_WINDOW` from the Windows API.
        const NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(NO_WINDOW);
    }
    command
}
