#!/usr/bin/env bash
#
# Regenerates every icon size from `parachron-1024.png`, which is the master and
# the only file here drawn by hand. Run it from anywhere; it writes beside itself.
#
# Three things here are deliberate and none of them is obvious from the output.
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
# **The art does not reach the edge.** Nearly every other app icon on a desktop
# ships a few transparent pixels of its own, so an icon drawn full bleed sits at
# the same nominal size as its neighbours and still reads larger and heavier,
# with corners the task manager's own rounding appears to clip. Each size is
# therefore resized to ICON_INSET_PCT of its box and centred in the rest. The
# margin is the whole point: it is what puts this icon at the same visual weight
# as everything else on the panel. That margin is also why both sources are cut
# to a rounded rectangle and given alpha outside it first: the master paints its
# tile on a near-black backdrop, and margin around an opaque square only draws
# attention to the square.
#
# The crop numbers are measured off the master, and they have to be re-measured
# whenever the master is redrawn. The first set here was not: it was carried over
# from the previous artwork and kept working well enough to look deliberate. On
# this master it began at y=30 — eighty pixels *above* the tile's own rim at
# y=110 — so every icon below the wordmark floor carried a dead band of backdrop
# across its top and wore the hexagon pushed off centre beneath it. On a task bar
# that reads as an icon that has been cut off, which is exactly what it was.
#
# Measured against this master: the tile's rim runs 110..910 on both axes, and
# its corner arc crosses the 45-degree diagonal about 60px in, which puts a
# circular radius at 60/(1 - 1/sqrt2) ~= 205. An 800-square from 110 gives up
# only the last and dimmest column of that rim. Inside it the hexagon spans
# roughly x 310..715 and y 210..665, centred on (512, 437), and the wordmark
# starts at y~705 — so a 520-square centred on the hexagon ends at y=697, which
# clears the text by a hair and fills the icon with the mark rather than with
# the plate around it.
set -euo pipefail
cd "$(dirname "$0")"

MASTER=parachron-1024.png
MARK_CROP="520x520+252+177"
MARK_SIZE=520
MARK_RADIUS=94            # ~18% of 520, matching the tile's own corner radius
TILE_CROP="800x800+110+110"
TILE_SIZE=800
TILE_RADIUS=205           # the arc the painted rim itself follows; see above
WORDMARK_FLOOR=96         # sizes below this use the mark; this and up use the tile
ICON_INSET_PCT=84         # art fills this much of the box; the rest is transparent margin

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Cut $1 out of the master at crop $2, then make everything outside a $3-square
# rounded rectangle of radius $4 transparent, leaving $work/$1-rounded.png.
rounded_crop() {
  magick "$MASTER" -crop "$2" +repage "$work/$1.png"
  magick -size "${3}x${3}" xc:black -fill white \
    -draw "roundrectangle 0,0,$(( $3 - 1 )),$(( $3 - 1 )),$4,$4" -alpha off "$work/mask.png"
  magick "$work/$1.png" -alpha off "$work/mask.png" \
    -compose CopyOpacity -composite "$work/$1-rounded.png"
}

# The mark, cut out of the master and given the tile's corners back.
rounded_crop mark "$MARK_CROP" "$MARK_SIZE" "$MARK_RADIUS"
# The tile, cut to its painted rim so its own corners are what the alpha follows.
rounded_crop tile "$TILE_CROP" "$TILE_SIZE" "$TILE_RADIUS"

for n in 16 24 32 48 64 96 128 256 512; do
  if [ "$n" -lt "$WORDMARK_FLOOR" ]; then
    src="$work/mark-rounded.png"
  else
    src="$work/tile-rounded.png"
  fi
  inner=$(( n * ICON_INSET_PCT / 100 ))
  [ "$inner" -ge 1 ] || inner=1
  magick "$src" \
    -colorspace RGB \
    -filter Lanczos -resize "${inner}x${inner}" \
    -colorspace sRGB \
    -background none -gravity center -extent "${n}x${n}" \
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
