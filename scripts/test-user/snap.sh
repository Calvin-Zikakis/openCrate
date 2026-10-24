#!/bin/zsh
# snap.sh <app> <out.png>: screenshot the app's front window, scaled so one image pixel is one
# screen point. Prints the window's origin: screen point = origin + pixel in the image.
set -euo pipefail
HERE=${0:A:h}
WINDOWS=$(swift $HERE/windows.swift "$1" 2>/dev/null | grep "onscreen: true" | grep "layer: 0" || true)
# A titled window first: pop-ups such as autocorrect suggestions are untitled windows of the app.
LINE=$(print -r -- $WINDOWS | grep -v '|  |' | head -1 || true)
[[ -n $LINE ]] || LINE=$(print -r -- $WINDOWS | head -1)
[[ -n $LINE ]] || { print "No window for $1 on screen."; exit 1; }
ID=${LINE%% *}
X=$(print -r -- $LINE | grep -o '"X": [0-9-]*' | grep -o '[0-9-]*$')
Y=$(print -r -- $LINE | grep -o '"Y": [0-9-]*' | grep -o '[0-9-]*$')
W=$(print -r -- $LINE | grep -o '"Width": [0-9]*' | grep -o '[0-9]*$')
FULL="${TMPDIR:-/tmp}/snap-full.png"
screencapture -x -o -l "$ID" $FULL
sips --resampleWidth "$W" $FULL --out "$2" >/dev/null
print "origin $X,$Y"
