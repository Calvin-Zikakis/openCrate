# Agent instructions

Read `docs/HANDOFF.md` first, then the newest entry in `docs/LOG.md`. The decisions live in `docs/DJ Library Manager — Game Plan.md`. Everything under `docs/` stays on this computer: the folder is ignored by git, so put notes, plans and guides there, not in the repository, and do not link to it from tracked files.

## Rules

1. Document as you go. After each step add a dated entry at the top of `docs/LOG.md`. Every call the plan does not settle goes in `docs/ASSUMPTIONS.md`, each choice of approach gets a decision record in `docs/decisions/`, and each measurement a research note in `docs/research/` that names the command and the dataset. When a decision changes, update the Game Plan.
2. Keep the tracked docs true. When a crate's public API or behaviour changes, update its `README.md`; when something a user sees changes, update the root `README.md`. After substantive work, delegate to the `documenter` agent ([.github/agents/documenter.agent.md](.github/agents/documenter.agent.md)) to reconcile the docs with the code.
3. No emojis in documentation. Use plain status words: done, planned, later.
4. Never touch the owner's real library. That means the NAS, Navidrome, rekordbox and the download folders. Development uses synthetic data and temp directories.
5. Keep private material out of the repository. No audio, no track names from the owner's library, no tokens or API keys.
6. Ask before committing or pushing, unless the owner has just told you to. The repository is private (Calvin-Zikakis/openCrate on GitHub). Do not change that.
7. Keep the lints clean and measure before claiming speed. See the workspace `Cargo.toml` and decision 0004 in `docs/decisions/`.
8. Save GitHub Actions minutes. CI runs only for a pull request into `main` or when started by hand, and the Installers workflow only by hand. Run the checks locally before every commit, and do not start a workflow unless the owner asks.
