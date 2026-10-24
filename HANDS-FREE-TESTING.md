# Letting Claude drive the app on its own

Claude tests openCrate hands-free from a separate, standard macOS account on the laptop. In that
account it can start openCrate and rekordbox, click and type, read the screen, and work with test
music, while your own account, files, keychain and logins stay out of its reach. Setup takes about
half an hour once; after that, starting a night of testing is a user switch and one command.

Why a separate account: with screen control on, Claude can click into any app that is open, and in
your own account that means everything you are signed in to. A standard account has its own files,
keychain and browser, cannot use `sudo`, and, once your home folder is closed to other users (step
2 below), cannot read your files.

## On your own account

1. **Create the account.** System Settings, Users & Groups, Add User. Choose **Standard**, full name
   `openCrate Test`, account name `octest`, and a password you keep.

2. **Close your home folder to other users.** In Terminal:

   ```
   chmod 700 ~
   ```

   By default macOS lets other accounts read most of a home folder. This stops that. Undo with
   `chmod 755 ~` if ever needed.

3. **Make a GitHub token for the openCrate repository only.** github.com, Settings, Developer
   settings, Fine-grained tokens, Generate new token:
   - Repository access: Only select repositories, `Calvin-Zikakis/openCrate`.
   - Permissions: Contents, Actions, Workflows and Pull requests set to Read and write.
   - Expiration: 90 days.

   Keep it for step 5. If the test account were ever misused, this token can reach that one
   repository and nothing else.

4. **Take screen control away from VS Code.** System Settings, Privacy and Security,
   Accessibility: turn **Visual Studio Code** off. macOS keeps this list per app for the whole Mac,
   not per account, so the test account will use Terminal instead. Do not leave an AI agent running
   in Terminal in your own account.

The preparation folder `/Users/Shared/openCrate-setup` (the repository, copies of the test music
and Claude's notes) is made from your account by Claude before the switch. Delete it from your
account once setup is done.

## In the test account

5. **Run the setup script.** Log in as `octest` (Apple menu, Lock Screen, Switch User; your own
   session stays open underneath). Open Terminal and run:

   ```
   zsh /Users/Shared/openCrate-setup/repo/scripts/test-user/setup.sh
   ```

   It copies the repository and test music, installs Rust, the test browsers and Claude Code, and
   asks for the GitHub token. At the end macOS asks three things on behalf of Terminal: allow it to
   control System Events (Allow), to record the screen (Allow), and to use Accessibility (turn
   Terminal on in the System Settings page that opens; it asks for your administrator name and
   password). Quit and reopen Terminal afterwards.

6. **Sign Claude Code in.** In Terminal, run `claude` once and follow the sign-in. Then type
   `/exit`.

7. **Give rekordbox its own library.** Open rekordbox from Applications, sign in if it asks, and let
   it create an empty library. Quit it. This library belongs to the test account and starts empty;
   Claude fills it with the test music.

## Each night

8. Switch to `octest`, open Terminal, and run:

   ```
   cd ~/repositories/openCrate && claude --dangerously-skip-permissions
   ```

   Then paste:

   > You are in the openCrate Test macOS account, set up for hands-free testing. Read
   > docs/HANDOFF.md, the newest entry in docs/LOG.md, and HANDS-FREE-TESTING.md, then carry on
   > with the next task there. Run `caffeinate -d -i -s` in the background first so the Mac stays
   > awake.

   Skipping permission prompts is acceptable here because the account itself is the boundary.

9. Leave the Mac on the test account's screen, plugged in, lid open. Screen control only works on
   the account that is showing; if you switch back to yours, Claude carries on with code work only.

## What Claude does there

- Starts openCrate with its own library: `OPENCRATE_DATA=~/Desktop/ocTest/data`, opened with
  `open -n --env OPENCRATE_DATA=... openCrate.app`.
- Imports from `~/Desktop/ocTest/Downloads` into `~/Desktop/ocTest/Music`, checks the editor,
  Clean up, duplicates and the rekordbox sync, and confirms each one in rekordbox by eye.
- Clicks with `cliclick`, sends keys through System Events (`osascript`), takes window screenshots
  with `screencapture -l`, and scrolls with a small CoreGraphics helper.
- Writes what it finds to `docs/owner-notes.md`, fixes it, and commits to the working branch.

It does not type passwords, approve system prompts, or touch anything outside the test account. If
macOS asks for something unexpected, it leaves the prompt and switches to code work.

## Removing it

System Settings, Users & Groups, select `openCrate Test`, Delete User. Then revoke the GitHub token
and turn Terminal off under Accessibility.
