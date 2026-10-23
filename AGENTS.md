# Agent instructions

Read [docs/HANDOFF.md](docs/HANDOFF.md) first, then the newest entry in [docs/LOG.md](docs/LOG.md). The decisions live in [DJ Library Manager — Game Plan.md](<DJ Library Manager — Game Plan.md>).

## Rules

1. **Document as you go.** After each step add a dated entry at the top of `docs/LOG.md`. Add an item to `docs/ASSUMPTIONS.md` for every call the plan does not settle. Add a decision record in `docs/decisions/` for each choice of approach, and a research note in `docs/research/` for each measurement (name the command and the dataset). Update the Game Plan when a decision changes.
2. **Use the documenter agent.** After substantive work, delegate to the `documenter` agent ([.github/agents/documenter.agent.md](.github/agents/documenter.agent.md)) to reconcile the docs with the code.
3. **No emojis in documentation.** Use plain status words: done, planned, later.
4. **Never touch the owner's real library.** That means the NAS, Navidrome, rekordbox and the download folders. Development uses synthetic data and temp directories.
5. **Keep private material out of the repository.** No audio, no track names from the owner's library, no tokens or API keys.
6. **Ask before committing or pushing, unless the owner has just told you to.** The repository is private (Calvin-Zikakis/openCrate on GitHub). Do not change that.
7. **Keep the lints clean and measure before claiming speed.** See the workspace `Cargo.toml` and decision 0004.
