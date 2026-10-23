# 0004: Interface and speed budgets

Status: accepted 2026-10-22. These come from the owner's stated requirements; the budget numbers are starting values.

## Context

The owner wants a pretty interface in the style of Spotify, not rekordbox's dense panels, and wants nothing to feel laggy.

## Decision

- Layout and feel follow Spotify: dark, artwork first, a left sidebar, search on top, a player bar at the bottom. No Spotify colors, logo or fonts (assumption A16).
- Plain CSS with design tokens, the system font stack, and no component library. React 19, TanStack Virtual for every list.
- Budgets, enforced by a benchmark on a synthetic 100,000-track library:

  | Interaction | Budget |
  | --- | --- |
  | Cold start to a usable list | under 1 s |
  | Scrolling | 60 fps |
  | Search after a keystroke | under 50 ms at the 95th percentile |
  | Open a release or artist | under 100 ms |
  | Play to first audio | under 150 ms for local or cached files |
  | Click feedback | within 100 ms |

- Only a window of rows crosses the IPC boundary. Heavy work runs in background workers with progress events and cancellation. Animations touch transform and opacity only.

## Reasoning

A page that renders 100,000 rows, or waits on a disk, a network or a database writer, will stutter whatever the styling. The budgets make "not laggy" a test instead of an impression.

## Consequences

- The data layer exposes windowed reads (`start`, `count`, a known `total`), never whole tables.
- The UI talks to a `LibraryApi` interface with a Tauri implementation and a mock implementation, so it can be developed and measured in a browser.
- Budgets are measured and the results recorded in research notes (a note on the database benchmark is planned). Tune them after measuring, in their own change.
