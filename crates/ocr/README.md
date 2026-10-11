# Local scan processing

`cargo xtask models` downloads the hash-pinned, openly licensed models in
`ATTRIBUTION.toml`. No document is uploaded and no model is downloaded at runtime.
Release packages ship these files beside the application.

- English, German, French, Spanish and horizontal Japanese use PaddlePaddle's
  PP-OCRv6 small model through the existing Rust RTen runtime. The legacy English
  recognizer API remains available to existing callers.
- PP-LCNet classifies quarter-turn orientation. Preprocessing centres on ink bounds,
  resizes the short side to 256, crops to 224 and uses the model's RGB normalization.
  Only predictions at or above 0.90 confidence are applied.
- Deskew searches projection profiles at bounded resolution over ±12 degrees. Weak,
  blank and dense-image results stay unchanged. Fine correction scales the whole page
  to fit inside its original bounds; it does not crop or recompress its images.
- OCR operates on corrected pixels, maps recognized boxes back to source coordinates,
  then transforms the PDF content and invisible Unicode text together. The engine
  uses empty Type 3 glyphs with ToUnicode mappings, so OCR needs no Japanese font asset.

The UI exposes independent rotation, deskew and searchable-text switches. Automation:
`ocr_recognize` and `ocr_recognize_files` accept `language`, `auto_rotate`, `deskew`
and `recognize`; `ocr_enhance` corrects scans without adding text. Rotation/deskew are
enabled by default. Disable both for exact-image output. Existing text pages are
skipped by default. Corrections and OCR form one undo step; stale worker results and
cancelled UI runs do not modify the document.

Limits: no perspective correction/dewarping, no guarantee on photos or ambiguous text,
no vertical Japanese claim, and deskew refuses annotated pages rather than separating
annotations from their targets. Supported OCR languages are deliberately limited to
those exposed and tested, despite the model's larger vocabulary.

Validation: `cargo test -p pdfcraft-engine --test scan_ocr` covers actual models,
languages, all quarter-turns, deskew, unchanged exact-mode pixels, save/reopen, undo
and stale results. Model tests report a skip when models are absent. The Japanese
synthetic fixture uses the optional release `CRAFT_FONTS_DIR` checkout; generated
fixtures and third-party document samples are not committed.
