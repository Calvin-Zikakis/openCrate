# 0007: Windows first, developed on a Mac

Status: proposed 2026-10-22 and recorded in the Game Plan. The platform facts and the Windows-first order come from the owner; the CI and testing design is a proposal. CI runs on GitHub Actions in a private repository (assumption A38).

## Context

The owner's music, rekordbox and library are all on a Windows desktop (the XML shows drive letters G:, D:, C: and E:). The Mac laptop is used only for development, and holds no music. The plan already says macOS and Windows behave identically, but nothing can be run on Windows from the dev machine.

## Decision

1. Windows is a first-class target from the first milestone. Every change is built and tested on both Windows and macOS in CI, and each CI run produces an unsigned Windows installer that the owner can try on the desktop.
2. Code that touches paths is OS-independent and tested with Windows-style input on every OS:
   - The sanitizer always applies the strictest rules of both OSes.
   - `rel_path` is stored with forward slashes, whatever the host.
   - Drive letters, UNC paths and the `file://localhost/` form are parsed and written by pure functions, not by calls that only exist on one OS.
3. The 260-character Windows path limit is enforced by `organize` unless long-path support is detected. A library root such as `G:\Music\<folder>\` already uses about 28 characters of it, and the path template adds three components.
4. Anything that needs the real library, rekordbox or the NAS is run by the owner on the desktop, from a script or a button that prints aggregate numbers only (counts, timings, pass or fail), never names.
5. Windows-only calls (drive detection, the long-path flag, WebView2 handling in the installer) sit behind a small interface with a fake for tests.

## Reasoning

- A bug that only shows on Windows would otherwise be found by the owner, on the one machine that matters, after the work is "done".
- Pure path functions can be tested thoroughly on the Mac. Only the thin OS layer needs the CI Windows runner.

## Consequences

- CI runs on GitHub Actions in a private repository (assumption A38). Free-plan minutes are limited and macOS runs count for more, so the Windows job runs on every push and the macOS job on pull requests; tune this after seeing the real usage.
- Milestone 0 includes CI and the installer artifact ([BUILD-PLAN](../BUILD-PLAN.md)).
- The test suite uses Windows-style fixtures (`G:\Music\...`, trailing dots, reserved names such as `CON`) even when running on macOS.
- Installer details to check on a clean Windows machine: whether the WebView2 runtime is present or gets installed by the Tauri installer.
