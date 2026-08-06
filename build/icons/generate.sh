#!/usr/bin/env bash
#
# Regenerates every icon size from `parachron-1024.png`, which is the master and
# the only file here drawn by hand. Run it from anywhere; it writes beside itself.
#
# Two things here are deliberate and neither is obvious from the output.
#
# **Linear light.** Resampling in sRGB averages gamma-encoded numbers, which dims
# bright-on-dark detail. That is this artwork's cyan glow specifically, so every
# resize converts to linear RGB first and back afterwards.
#
# **The wordmark comes off below 96px.** The master is a tile with PARACHRON
# lettered across the bottom. At 32px those letters are about four pixels tall —
# no filter makes four-pixel letters legible, and the smear they leave drags the
# whole icon down. So the small sizes are cut from the mark alone and the large
# ones keep the full tile, which is what a task bar and an About pane
# respectively need. The freedesktop icon theme looks each size up separately,
# so this costs nothing to express.
#
# The crop numbers were measured off the master with a coordinate overlay:
# wordmark y 704..810, hexagon y 90..660, hexagon x 240..800 (centre x 520).
# A 670-square ending at y=700 clears the text with room to spare.
set -euo pipefail
cd "$(dirname "$0")"

MASTER=parachron-1024.png
MARK_CROP="670x670+185+30"
MARK_RADIUS=120           # ~18% of 670, matching the tile's own corner radius
WORDMARK_FLOOR=96         # sizes below this use the mark; this and up use the tile

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The mark, cut out of the master and given the tile's corners back.
magick "$MASTER" -crop "$MARK_CROP" +repage "$work/mark.png"
magick -size 670x670 xc:black -fill white \
  -draw "roundrectangle 0,0,669,669,$MARK_RADIUS,$MARK_RADIUS" -alpha off "$work/mask.png"
magick "$work/mark.png" -alpha off "$work/mask.png" \
  -compose CopyOpacity -composite "$work/mark-rounded.png"

for n in 16 24 32 48 64 96 128 256 512; do
  if [ "$n" -lt "$WORDMARK_FLOOR" ]; then
    src="$work/mark-rounded.png"
  else
    src="$MASTER"
  fi
  magick "$src" \
    -colorspace RGB \
    -filter Lanczos -resize "${n}x${n}" \
    -colorspace sRGB \
    -strip \
    "PNG32:parachron-${n}.png"
done

# The Windows build wants a single multi-resolution .ico (CORE §7).
magick \
  parachron-16.png parachron-24.png parachron-32.png parachron-48.png \
  parachron-64.png parachron-128.png parachron-256.png \
  parachron.ico

echo "regenerated from $MASTER:"
for n in 16 24 32 48 64 96 128 256 512; do
  [ "$n" -lt "$WORDMARK_FLOOR" ] && kind=mark || kind=tile
  printf "  %-4s %-5s %s\n" "$n" "$kind" "$(magick identify -format '%wx%h %B bytes' "parachron-${n}.png")"
done
printf "  ico       %s frames\n" "$(magick identify parachron.ico | wc -l)"
