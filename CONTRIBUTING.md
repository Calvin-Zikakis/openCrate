# Contributing

Thanks for taking an interest in openCrate. It is an early test version with one maintainer, so the best first
contributions are bug reports from your own library and small, focused pull requests.

## Reporting a problem

Open an issue with:

- what you did, what you expected and what happened, in plain words;
- your platform (Windows or macOS, and the version), and the rekordbox version if it was involved;
- whether the music was in a music folder, a watch folder, or rekordbox, and the file format;
- the text of any message the app showed.

Never attach your library database, a rekordbox backup or real audio files. If a specific file makes the app
misbehave, say which format and which tag layout; the maintainer can usually build a synthetic one that matches.

## Before you change code

The [README](README.md#development) has the setup. Build the frontend first, then run every check:

```sh
npx -y -p node@24 -p npm@11 npm ci
npx -y -p node@24 -p npm@11 npm run build
npx -y -p node@24 -p npm@11 npm test
npx -y -p node@24 -p npm@11 npm run test:browser
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Clippy runs with warnings as errors, and the lint set in the workspace `Cargo.toml` is strict on purpose. Fix what
it reports; where a lint is wrong for one spot, use `expect` with a reason, not a blanket `allow`.

## What a change should look like

- One change per pull request, with a commit message that says what the app now does differently.
- A test for every change in behaviour: a Rust test beside the code, a Vitest unit test for frontend logic, or a
  Playwright test in `tests/browser` for something a person sees.
- Tests use synthetic audio (`oc_tag::fixtures`), temporary folders and made-up names. No real recordings, no real
  library databases, no file paths from a real computer.
- The app never changes a file the person did not ask it to change. If your change writes to a music folder, a
  watch folder or rekordbox's library, say so in the pull request and explain how the person chooses it.
- No emojis in documentation, and the docs describe what the code does today, not what is planned.

## Continuous integration

The checks run on GitHub for every pull request into `main`, and a pull request is merged only once they pass.
The Installers workflow builds unsigned Windows and macOS installers and runs by hand only. Run the checks
locally before you push.

## Scope

rekordbox owns analysis (BPM, key, beat grids, cues) and playlists; openCrate reads them and does not try to
replace them. Serato, Traktor and Engine support are not planned for this version. If you want to start on
something larger than a fix, open an issue first so the shape can be agreed before the work.
