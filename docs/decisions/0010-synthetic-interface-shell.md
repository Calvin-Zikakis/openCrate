# 0010: Synthetic interface shell before filesystem indexing

Date: 2026-10-22.
Status: accepted for implementation under the owner's instruction to continue.

## Decision

Build the visual part of milestone 0e against a deterministic, in-memory
`LibraryApi` before 0d's filesystem identity and library schema. This is allowed
by the mock API contract in [interface](../design/interface.md); it does not
settle the path or availability contracts raised in either review.

Use React 19, TypeScript 5.9, Vite 7, TanStack Virtual 3 and plain CSS with the
existing dark/orange tokens. Pin Node 24 for the frontend, use npm and its
lockfile, and run DOM tests in an actual Chromium browser through Playwright.
Keep Rust 1.99.0. Use a minimal Tauri 2 host without filesystem, shell or network
plugins. Do not connect the lifecycle database to a UI command prematurely.

A dedicated Web Worker owns the synthetic catalog, filtering and ordering.
Only bounded result windows cross its message boundary. Requests have IDs,
cancellation and stale-result guards. The interface says it is a demo; no
generated titles represent the owner's library. Playback, real folders,
playlist import and provider settings must not pretend to work.

The initial interface includes a sidebar, search, virtualized track and release
views, artist filtering, remembered appearance preferences, empty import/sync
screens and a disabled player preview. Broader product settings arrive with the
features that use them, especially the native download-folder picker.

## Acceptance and limits

Test 100,000 synthetic tracks: deterministic sorted/filtered windows, bounded
DOM rows/cards, deep scrolling, keyboard access, result-race handling, storage
failures, contrast and reduced motion. Record browser workload, sample counts,
warm/cold state and observed timings. Browser mock numbers do not establish
Tauri IPC/database performance or the owner's Windows budgets.

Build the web bundle and a native development host if the available toolchain
supports it. Configure frontend CI and an on-demand unsigned Windows build;
do not claim Windows or installer validation before those run.

Estimated remaining engineering/review work for this slice: 12-24 hours,
excluding owner Windows checks; an estimate, not a measured schedule. Split out
native packaging or a failing performance workload rather than blocking a
tested browser shell. No real-library reads, writes, audio or online services.
