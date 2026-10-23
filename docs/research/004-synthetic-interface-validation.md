# 004: Synthetic interface validation

Date: 2026-10-22.

## Scope and provenance

This records the parent's local execution evidence for
[decision 0010](../decisions/0010-synthetic-interface-shell.md). The documentation
pass inspected frontend source, manifests/lockfile, tests, Tauri main/config/
capabilities and workflows. It did not rerun commands or inspect native
rendering. The parent viewed track/release screenshots in ignored `test-results/`.
No screenshot or generated report is committed by this documentation pass.

The visual sub-slice is built locally, not full milestone 0. The demo uses an
in-memory worker, not a windowed SQLite API or real database IPC. It never
accesses `oc-core-db`, music, a private XML export, download folders or audio.

## Environment and commands

- arm64 macOS development machine; no Windows execution. A specific Mac model
  and OS build were not supplied with the evidence.
- Observed Node **24.22.1**, npm **11.21.1**. `.nvmrc` selects major 24;
  `package.json` requires `>=24 <25`. These files do not pin those exact patches.
- Rust 1.99.0, with the Tauri host included in workspace checks. Cargo metadata
  confirmed two library crates plus `opencrate-desktop`; default members remain
  the two library crates.
- Headless Chromium **156.0.8078.4**, viewport **1440x960**, one Playwright
  test worker, no retries. The demo itself has one catalog Web Worker.
- React 19, Vite 7, TypeScript 5.9, TanStack Virtual 3, plain CSS.
  The inspected lockfile resolves React/React DOM 19.3.1, Vite 7.3.7,
  TypeScript 5.9.3, TanStack React Virtual 3.14.22, Vitest 5.0.5 and
  Playwright test 1.64.1.

Run from the repository root. The exact npm command form is
`npx -y -p node@24 -p npm@11 npm <...>`, not the global Node 26 installation.

| Command | Parent-reported result |
| --- | --- |
| `npx -y -p node@24 -p npm@11 npm ci` | Passed; lockfile reproducibility checked; zero audit findings |
| `npx -y -p node@24 -p npm@11 npm run build` | TypeScript no-emit check and Vite production build passed |
| `npx -y -p node@24 -p npm@11 npm test` | 12 unit tests passed in three files |
| `npx -y -p node@24 -p npm@11 npm run test:browser` | Nine Chromium tests passed; measurements below |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 25 tests passed: 18 database, seven codec; zero Tauri tests and zero doctests |
| `cargo check --workspace --locked` | Passed, including the Tauri host |
| `npx -y -p node@24 -p npm@11 npm run tauri -- build --debug --bundles app` | Produced the arm64 macOS development app described below |

The production frontend must exist before direct workspace Cargo checks because
Tauri embeds `dist/`. For a fresh machine with missing Chromium, use
`npx -y -p node@24 -p npm@11 npm exec -- playwright install chromium`.
That is a setup instruction, not an additional claimed measurement.

The newly added Vitest 3 graph had audit warnings. The implementation deliberately
updated to Vitest 5 rather than applying an automatic audit fix. The current
install reports zero findings, not a guarantee of dependency security.
`npm ci` warned about unapproved esbuild/fsevents install scripts; builds still
passed. Do not silently approve extra scripts to remove the warning.

## Fixtures and test coverage

[`createCatalog()`](../../src/library/catalog.ts) deterministically constructs
100,000 tracks, 10,000 releases and 240 artists. Each full release has ten tracks.
Metadata, format labels, BPM/key values and CSS artwork are generated, not read
from files. Smaller unit fixtures cover zero tracks and partial releases.
No private track names or audio are part of this dataset.

| Unit test file | Count and scope |
| --- | --- |
| [`catalog.test.ts`](../../src/library/catalog.test.ts) | Six: exact totals, first/deep windows, repeated filtering, stable sort/null BPM, release/artist membership, formats and invalid bounds |
| [`worker-api.test.ts`](../../src/library/worker-api.test.ts) | Three: request correlation/cancellation and stale replies, worker failure, invalid/already-aborted requests |
| [`preferences.test.ts`](../../src/preferences.test.ts) | Three: absent defaults, validated round-trip/malformed storage, read/write failures |

Vitest runs in the Node environment, not a simulated DOM. Actual DOM tests are
in [`tests/browser`](../../tests/browser/) using
[`playwright.config.ts`](../../playwright.config.ts) and the production Vite
preview on loopback port 1421. Eight behavioral tests plus one measurement test
cover:

1. Bounded track DOM, deep scrolling and selection.
2. Search races, format filters, sorting and empty results.
3. Virtualized release/artist cards and correct track drill-down.
4. `/`, row focus/End navigation, modal dismissal/focus return and appearance
   persistence.
5. Visible storage write failure and explicit unbuilt import/XML/player states.
6. Minimum-width layout at 720x600 without horizontal overflow.
7. Worker-construction failure producing a visible error rather than a blank UI.
8. All twelve main/secondary/faint foreground versus four-surface combinations
   at least 4.5:1, plus system reduced motion.
9. The named synthetic latency/scroll workload and local design screenshots.

The faint token is now `#9494a1`. The old design value `#80808c` is superseded.
This token matrix is not a complete accessibility or composited-color audit.
Collapsed sidebar controls retain `aria-label` names in code.

## Latest browser measurements

Command: `npx -y -p node@24 -p npm@11 npm run test:browser`.
Measurement code:
[`performance.spec.ts`](../../tests/browser/performance.spec.ts).
Dataset: the full deterministic catalog above, default recent ordering,
1440x960 headless Chromium on the named arm64 Mac/Node 24 environment.

| Measurement | Latest recorded value | Samples |
| --- | --- | --- |
| `coldReadyObservedMs` | 83.199999988 ms (about 83.2 ms) | One observed page readiness |
| `searchP95Ms` | 34.100000024 ms (about 34.1 ms) | 30 warm searches |
| `scrollFrameP95Ms` | 17.5 ms | 120 frame intervals |
| `scrollMaxFrameMs` | 17.800000012 ms (about 17.8 ms) | Same 120 intervals |
| `scrollFramesOver33Ms` | 0 | Same 120 intervals |
| `maximumRenderedTrackRows` | 21 | Maximum observed during that scroll workload |

These latest values are authoritative for this checkpoint. The earlier parent
run reported 35.1 ms search p95, 18.3 ms scroll p95 and 19 ms maximum; it is
superseded, not pooled with this run or evidence of a measured speedup.

### Definitions and limits

- **Cold observed readiness:** after navigation, Playwright waits for the
  status to contain the full track count, then reads page `performance.now()`.
  This includes test polling and scheduling. It is not a precise interval from
  process startup or first navigation instruction to paint.
- **Warm search:** 30 input events cycle through six synthetic queries,
  including no-result and unique-result cases. Timing begins immediately before
  setting the input and dispatching its event. Completion requires the current
  query's status and `aria-busy=false`, confirmed across two animation frames.
  This measures input event through current-result completion plus two rAF
  observations, not only query execution or a precisely instrumented paint.
  The catalog and default sort index are already warm.
- **Scroll:** after clearing search and restoring the full list, the script
  records 120 rAF intervals and sets `scrollTop = index * 560`. It counts
  rendered track-row elements each iteration. These intervals include scheduling
  and script work; they do not guarantee physical frame presentation or cover
  every deep row. A separate behavioral test reaches the list's end.
- **Percentiles:** values are sorted and p95 is the nearest-rank element at
  `ceil(n * .95) - 1`. The displayed decimal precision is timer output, not
  precision of a general startup/render guarantee.
- **Bounds:** production worker replies have at most 200 items. Browser tests
  assert fewer than 45 track rows and fewer than 60 release cards in their
  tested views. The observed maximum of 21 is not a universal DOM-row cap.
- **Cancellation:** abort rejects pending promises; outdated replies are ignored.
  There is no worker cancellation message or interruption of synchronous computation
  already running; this is not a CPU-cancellation benchmark.

The measurement test records latency/frame values and asserts the DOM bound.
It does not enforce a search p95 or frame-time pass threshold. No general speed
claim follows from a passing suite. There is no Windows, native webview,
SQLite/IPC, import-load, real-library, network or audio measurement here.

## Native artifact and security boundary

`npx -y -p node@24 -p npm@11 npm run tauri -- build --debug --bundles app`
produced `target/debug/bundle/macos/openCrate.app`. The parent reported
**14.78 MiB**, arm64 only, with a linker ad-hoc signature and no Developer Team.
This is the development bundle dataset, not a signed release or Windows
installer. The size/signature inspection invocations were not supplied.

The exact executable
`target/debug/bundle/macos/openCrate.app/Contents/MacOS/opencrate-desktop`
was started, remained alive after three seconds, and was stopped. Only process
liveness was checked. There is **no native UI readiness, rendering or
responsiveness evidence**. No app or server was left running. Cleanup of native
stdout/stderr scratch logs remains the parent's task.

The host has no custom commands, filesystem/network/shell plugins or command
permissions. The configured CSP allows local assets/workers, data images and
Tauri's local IPC transport, with inline styles for virtualization/artwork.
It allows no remote service origin. These settings do not prove OS-level
isolation. The UI has no core-db dependency or connection. Only appearance
persists in `localStorage`; pickers/import/XML/playback remain disconnected.
Only generated desktop icons are retained, not mobile icons.

## CI and continuation

- [`rust.yml`](../../.github/workflows/rust.yml) now installs Node 24 and builds
  the frontend before Windows/macOS fmt, strict clippy, workspace tests/build.
- [`frontend.yml`](../../.github/workflows/frontend.yml) configures Linux web,
  unit and Chromium checks plus browser artifacts. Its unsigned Windows NSIS
  build runs only on manual dispatch.
- Both remote workflows are **not run**. No push/commit authorization exists.
  Static source inspection is not hosted-CI evidence.
- Next safe slice: standalone read-only rekordbox XML parsing against synthetic
  fixtures, not started. No matching, mirror database or UI connection. Keep
  path/state, scheduling and mutation gates in
  [HANDOFF](../HANDOFF.md#5-unresolved-review-gates).

For an owner preview, use
`npx -y -p node@24 -p npm@11 npm run tauri -- dev` with a demo-only warning.
Record native visual results separately and stop both app and server afterwards.
