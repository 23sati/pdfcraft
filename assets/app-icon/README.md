# XueLuoPDF app icon

<img src="xueluopdf-1024.png" alt="XueLuoPDF app icon: a blue-haired character with cat ears holding a bouquet of pink roses" width="128">

**Artwork:** a character portrait — blue hair, cat ears, a white bow with a small gold and blue
crown, and a bouquet of pink roses — filling the tile on a white field.

**Style:** full-bleed colour artwork in a rounded square, no frame or roundel. Windows and Linux
icons use the full-bleed tile (`rx=112` of 512); macOS icons put it on Apple's grid (an 824 px
body centred on a transparent 1024 px canvas).

**Provenance:** AI-generated artwork, directed and supplied by the project owner (delivered as the
1108 px PNG kept at the repository root as `Title.png`). It is contributor-original for this
project, licensed like the rest of it (`MIT OR Apache-2.0`, see [LICENSE.txt](LICENSE.txt)); no
third-party material is involved. `packaging/icons.sh` cuts the rounded tile, renders every size
and rebuilds the `.ico` and `.icns`.

**The source is a bitmap, not a vector.** The previous PdfCraft icon was a traced SVG; this artwork
has no vector master, so `xueluopdf-1024.png` (1024 px) is the master and every other file is a
resample of it. Fine detail (the flower cluster, the lace) reads as texture below about 48 px.

## Files

| File | What it is |
|---|---|
| `xueluopdf-1024.png` | the master (1024 px, opaque); every PNG, `.ico` and `.icns` is rendered from it |
| `xueluopdf.icns` | macOS icon (16–512 px plus `@2x`), Apple's grid |
| `xueluopdf.ico` | Windows icon (16–256 px), embedded in `xueluopdf.exe` by `apps/pdfcraft/build.rs` |
| `hicolor/<n>x<n>/apps/ai.storyteller.pdfcraft.png` | Linux hicolor theme, 16–512 px; the 256 px one is the runtime icon on Windows and Linux |
| `pdfcraft-document.svg` | Windows PDF file icon master: a Lucide document outline, green badge and outlined PDF lettering; no font dependency (kept from PdfCraft) |
| `pdfcraft-document.ico` | Windows PDF file icon (16–256 px), embedded as resource 2 in `xueluopdf.exe` |

Where it shows: `apps/pdfcraft/src/main.rs` sets the window icon (Dock, taskbar, Alt-Tab, launcher)
and the Wayland app id `ai.storyteller.pdfcraft` (the internal id keeps the upstream name, so an
existing installation's settings and file associations do not move); the hicolor icon is named by
`packaging/linux/ai.storyteller.pdfcraft.desktop`.

Windows PDF files use resource 2 (`"xueluopdf.exe",-2`) when XueLuoPDF is chosen as the default
reader; the app keeps resource 1. The document outline is adapted from Lucide `file-text`
1.49.0 (ISC); its badge and outlined lettering are original (`MIT OR Apache-2.0`), covered by
[LICENSE-document.txt](LICENSE-document.txt). Both icons are embedded, requiring no runtime
sidecar. This provides a file type icon; first-page thumbnails still need a thumbnail provider.

## Regenerate

```sh
packaging/icons.sh        # needs python3 with Pillow and (on macOS) iconutil for the .icns
cargo xtask assets        # then update the sha256 values in ATTRIBUTION.toml and run with --write
```
