#!/bin/zsh
# Builds the macOS app and disk image on this Mac, as the macOS job in .github/workflows/installers.yml does on
# GitHub. See BUILDING.md. The result is unsigned and meant for testing.
set -euo pipefail

REPO=${0:A:h:h:h}
cd $REPO

xcode-select -p >/dev/null 2>&1 || { print "Missing the Xcode Command Line Tools: run xcode-select --install."; exit 1; }
for tool in node npm cargo; do
  command -v $tool >/dev/null || { print "Missing $tool: see BUILDING.md."; exit 1; }
done

# Build with the Node major version CI uses (.nvmrc). If another one is installed, npx fetches the right one
# for this build only, as the README's commands do.
WANT=$(tr -d ' v\n' < .nvmrc)
WANT=${WANT%%.*}
HAVE=$(node --version)
HAVE=${${HAVE#v}%%.*}
if [[ $HAVE == $WANT ]]; then
  NPM=(npm)
else
  print "Node $HAVE is installed; this build uses Node $WANT through npx."
  NPM=(npx -y -p node@$WANT -p npm@11 npm)
fi

# Absolute, because the Tauri CLI runs cargo from src-tauri and a relative folder would land there.
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-target}
CARGO_TARGET_DIR=${CARGO_TARGET_DIR:a}

$NPM ci
$NPM run tauri -- build --bundles app,dmg

# The newest disk image, in case older versions are still in the folder.
DMG=($CARGO_TARGET_DIR/release/bundle/dmg/*.dmg(Nom[1]))
if (( ! $#DMG )); then
  print "The build finished, but no disk image was found in $CARGO_TARGET_DIR/release/bundle/dmg."
  exit 1
fi
print "\nDisk image: $DMG[1]"
print "App:        $CARGO_TARGET_DIR/release/bundle/macos/openCrate.app"
