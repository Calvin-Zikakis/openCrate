#!/bin/zsh
# One-time setup of the macOS account Claude uses for hands-free testing (see docs/HANDS-FREE-TESTING.md).
# Run it in Terminal while logged in as that account. It copies the repository, the test music and
# Claude's notes from /Users/Shared/openCrate-setup (prepared from the owner's account), installs the
# per-user tools (Rust, Playwright's browsers, Claude Code), signs gh in with a token limited to the
# openCrate repository, and asks macOS for the permissions that let Claude drive the screen.
# Running it again skips whatever is already done.
set -euo pipefail

SETUP=/Users/Shared/openCrate-setup
REPO=$HOME/repositories/openCrate
# Outside Desktop and Documents on purpose: those folders are guarded by macOS, and every rebuilt debug app
# would have to ask again for access to them.
TEST=$HOME/ocTest

step() { print -P "\n%B== $1%b"; }

[[ -d $SETUP ]] || { print "Missing $SETUP: prepare it from the owner's account first."; exit 1; }
if [[ $(id -Gn) == *admin* ]]; then
  print "Warning: this account is an administrator. A standard account is safer."
fi

step "Paths"
# Homebrew lives in the owner's account but its tools work for everyone; Rust and Claude Code are per user.
touch ~/.zprofile
grep -q 'brew shellenv' ~/.zprofile || print 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zprofile
grep -q '.local/bin' ~/.zprofile || print 'export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"' >> ~/.zprofile
eval "$(/opt/homebrew/bin/brew shellenv)"
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

step "Repository"
if [[ ! -d $REPO ]]; then
  mkdir -p ${REPO:h}
  ditto $SETUP/repo $REPO
fi
print "$REPO"

step "Test music"
mkdir -p $TEST/Music $TEST/data
[[ -d $TEST/Downloads ]] || ditto $SETUP/music $TEST/Downloads
print "$(find $TEST/Downloads -type f | wc -l | tr -d ' ') files in $TEST/Downloads"

step "Claude's notes"
mkdir -p ~/.claude
[[ -f ~/.claude/CLAUDE.md ]] || cp $SETUP/claude/CLAUDE.md ~/.claude/CLAUDE.md
# Claude Code keys a project's memory by its folder, with each / written as -.
MEMORY=~/.claude/projects/${REPO//\//-}/memory
mkdir -p $MEMORY
cp -n $SETUP/claude/memory/* $MEMORY/ 2>/dev/null || true
print "$MEMORY"

step "Rust"
if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none --profile minimal --no-modify-path
fi
CHANNEL=$(sed -n 's/^channel = "\(.*\)"/\1/p' $REPO/rust-toolchain.toml)
rustup toolchain install $CHANNEL --profile minimal --component clippy,rustfmt
cargo --version

step "Node packages and test browsers"
(cd $REPO && npm ci --no-audit --no-fund && npx playwright install chromium webkit)

step "Claude Code"
command -v claude >/dev/null || curl -fsSL https://claude.ai/install.sh | bash
claude --version

step "GitHub"
if ! gh auth status >/dev/null 2>&1; then
  print "Paste the fine-grained token (limited to the openCrate repository), then press Return:"
  read -rs TOKEN
  print -r -- "$TOKEN" | gh auth login --hostname github.com --git-protocol https --with-token
  unset TOKEN
fi
gh auth setup-git
gh auth status

step "Screen permissions"
# Each of these makes macOS ask once, on behalf of Terminal. Choose Allow, and turn Terminal on
# where System Settings opens (it asks for an administrator's name and password).
osascript -e 'tell application "System Events" to get name of first process' >/dev/null 2>&1 || true
screencapture -x "${TMPDIR:-/tmp}/permission-check.png" 2>/dev/null && rm -f "${TMPDIR:-/tmp}/permission-check.png"
cat > "${TMPDIR:-/tmp}/ask-accessibility.swift" <<'SWIFT'
import ApplicationServices
let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
print(AXIsProcessTrustedWithOptions(options) ? "Accessibility: on" : "Accessibility: off; turn on Terminal in System Settings")
SWIFT
swift "${TMPDIR:-/tmp}/ask-accessibility.swift" || true
open "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"

step "Done"
print "Next: quit and reopen Terminal (permissions are read when it starts), open rekordbox once to"
print "make its library, quit it, then start Claude as docs/HANDS-FREE-TESTING.md describes."
