# Building the installers

Two scripts build openCrate's installers on your own computers, the same way the Installers workflow does on
GitHub, without using Actions minutes. What they make is unsigned and meant for testing, not a release.

## Windows

### Get the repository

In PowerShell (Windows key, type PowerShell):

```
winget install Git.Git
git clone https://github.com/Calvin-Zikakis/openCrate.git
cd openCrate
```

Skip the first line if Git is already installed; after installing it, open a new PowerShell window before
cloning. The repository is private, so the first clone opens a browser window to sign in to GitHub. To build a
branch other than `main`, run `git switch <branch>`; run `git pull` before each build to get the latest.

### Build

From the repository folder:

```
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\windows\build.ps1
```

Windows does not run PowerShell scripts by default. `-ExecutionPolicy Bypass` allows this one run and changes
no setting. Add any of these to the end of the line:

- `-Yes` installs missing tools without asking about each one.
- `-Test` runs the Rust tests (`cargo test --workspace --locked`, as Windows CI does) before building.
- `-TargetDir D:\b` uses another build folder instead of `C:\t`. Keep it short: OpenSSL's build fails on
  long paths, which is why the default is a folder at the root of the drive.

### One-time setup

The first run checks for each tool below. For anything missing it says what is wrong and offers to install it
with winget, asking before each one:

- Git.
- Node.js at the version in `.nvmrc` (24).
- Visual Studio 2022 Build Tools with "Desktop development with C++", which includes the Windows SDK. Rust and
  the C libraries are compiled with it. It needs administrator rights (Windows asks), several GB of disk, and
  the install takes a while. If Build Tools is already installed without that workload, the script says so:
  add it in Visual Studio Installer with Modify.
- Strawberry Perl in `C:\Strawberry`. The rekordbox library compiles OpenSSL from source, and its build needs
  Perl modules that other Perls on Windows (such as the one with Git) lack.
- Rust through rustup. The script then installs the pinned toolchain from `rust-toolchain.toml`.

Windows asks for administrator permission for most of these. WebView2, which the app runs in, comes with
Windows 11. ffmpeg is not needed to build; openCrate uses it when it converts formats on import
(`winget install ffmpeg`).

If a tool was installed but the script still cannot find it, close PowerShell, open a new window and run the
script again.

### The installer

The script prints its path at the end. With the default build folder it is:

```
C:\t\release\bundle\nsis\openCrate_<version>_x64-setup.exe
```

It installs for your user only and needs no administrator rights. It is unsigned, so SmartScreen warns when you
open it: choose More info, then Run anyway.

## Mac

The Mac needs the Xcode Command Line Tools (`xcode-select --install`), Node and Rust (rustup). From the
repository folder:

```
scripts/mac/build.sh
```

If the installed Node is not the `.nvmrc` version, the script runs npm through `npx` with the right Node, for
this build only. It prints where the disk image is:

```
target/release/bundle/dmg/openCrate_<version>_aarch64.dmg
```

The app itself is next to it, in `target/release/bundle/macos/openCrate.app`. If `CARGO_TARGET_DIR` is set, both
are under that folder instead of `target`. Copied to another Mac, the unsigned app is blocked the first time;
allow it under System Settings, Privacy and Security, Open Anyway.

## How long it takes

The first build compiles everything, including OpenSSL and SQLCipher, and takes a while; later builds reuse the
build folder and are quicker. On Windows the first run also installs the tools, and Tauri downloads its NSIS
installer maker once. Both need an internet connection. The Windows script prints how long the build took.
