#!/usr/bin/env bash
# Regenerate the app icons from assets/app-icon/xueluopdf-1024.png (the artwork master) and the
# Windows PDF file icon from assets/app-icon/pdfcraft-document.svg.
#
# Needs: python3 with Pillow (the master is a bitmap, so every size is a resample of it) and, only
# for the .icns, `iconutil` (macOS). The `.ico` is written here directly, so Windows and Linux
# never need a macOS tool. The outputs are committed, so building and packaging never need any of
# this. After running it, update the sha256 values in ATTRIBUTION.toml, then
# `cargo xtask assets --write`.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
MASTER="$DIR/xueluopdf-1024.png"
ID="ai.storyteller.pdfcraft"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v python3 >/dev/null || { echo "error: python3 not found" >&2; exit 1; }
python3 -c 'import PIL' 2>/dev/null || { echo "error: Pillow not found (python3 -m pip install Pillow)" >&2; exit 1; }
[ -f "$MASTER" ] || { echo "error: missing $MASTER" >&2; exit 1; }

python3 - "$MASTER" "$DIR" "$ID" <<'PY'
import io
import os
import struct
import sys

from PIL import Image, ImageDraw

master_path, out_dir, app_id = sys.argv[1], sys.argv[2], sys.argv[3]
RADIUS, SS = 112, 4  # rounded-tile corner radius of a 512 tile, and mask supersampling


def rounded(size, radius=RADIUS):
    """The master scaled to `size` and clipped to a rounded square (transparent outside)."""
    big = master.resize((size * SS, size * SS), Image.LANCZOS)
    mask = Image.new("L", (size * SS, size * SS), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size * SS - 1, size * SS - 1], radius=radius * SS, fill=255)
    big.putalpha(mask)
    return big.resize((size, size), Image.LANCZOS)


def apple_grid(size):
    """Apple's grid: an 824/1024 body centred on a transparent canvas."""
    body = int(round(size * 824 / 1024))
    tile = rounded(body)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    off = (size - body) // 2
    canvas.paste(tile, (off, off), tile)
    return canvas


def write_ico(path, sizes=(16, 20, 24, 32, 40, 48, 64, 128, 256)):
    """PNG-compressed entries. No 256 px entry: Pillow would have to store it uncompressed."""
    frames = [rounded(s) for s in sizes]
    frames[-1].save(path, format="ICO", append_images=frames[:-1], sizes=[(s, s) for s in sizes])


def write_icns(path):
    """The chunks `iconutil` would build from an iconset, written directly (PNG payloads)."""
    layout = [
        (b"icp4", 16), (b"icp5", 32), (b"icp6", 64),
        (b"ic07", 128), (b"ic08", 256), (b"ic09", 512),
        (b"ic10", 1024), (b"ic11", 32), (b"ic12", 64),
        (b"ic13", 256), (b"ic14", 512),
    ]
    body = b""
    for kind, size in layout:
        buf = io.BytesIO()
        apple_grid(size).save(buf, format="PNG")
        png = buf.getvalue()
        body += kind + struct.pack(">I", len(png) + 8) + png
    with open(path, "wb") as f:
        f.write(b"icns" + struct.pack(">I", len(body) + 8) + body)


master = Image.open(master_path).convert("RGBA")
if master.size != (1024, 1024):
    sys.exit(f"error: {master_path} must be 1024x1024, not {master.size[0]}x{master.size[1]}")

# Linux hicolor theme (full bleed; hicolor/256x256 is also the runtime icon on Windows and Linux).
for s in (16, 24, 32, 48, 64, 128, 256, 512):
    d = os.path.join(out_dir, "hicolor", f"{s}x{s}", "apps")
    os.makedirs(d, exist_ok=True)
    rounded(s).save(os.path.join(d, f"{app_id}.png"), optimize=True)

write_ico(os.path.join(out_dir, "xueluopdf.ico"))
write_icns(os.path.join(out_dir, "xueluopdf.icns"))
print(f"icons written to {out_dir}")
PY

# The Windows PDF file icon keeps its own vector master, which needs resvg (Lucide outline + badge).
DOC="$DIR/pdfcraft-document.svg"
if command -v resvg >/dev/null; then
  for s in 16 24 32 48 64 128 256; do
    resvg -w "$s" -h "$s" "$DOC" "$TMP/doc-$s.png" </dev/null
  done
  python3 - "$DIR/pdfcraft-document.ico" "$TMP"/doc-*.png <<'PY'
import struct, sys
out, pngs = sys.argv[1], sorted(sys.argv[2:], key=lambda p: int(p.rsplit("-", 1)[1].split(".")[0]))
blobs = [open(p, "rb").read() for p in pngs]
head = struct.pack("<HHH", 0, 1, len(blobs))
entries, data, offset = b"", b"", 6 + 16 * len(blobs)
for b in blobs:
    w, h = struct.unpack(">II", b[16:24])  # IHDR
    entries += struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, 32, len(b), offset)
    data += b
    offset += len(b)
open(out, "wb").write(head + entries + data)
PY
else
  echo "warning: resvg not found (cargo install resvg); pdfcraft-document.ico not regenerated" >&2
fi
