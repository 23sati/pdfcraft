# pdfcraft-print

Layer L4: printing (execution plan M10.5).

```rust
let pages = select_pages(count, Some("1-3, ii, 9-"), &labels, Subset::All, false)?;
let settings = Settings { pages, paper: PAPERS[0].1, layout: Layout::multiple(4), ..Default::default() };
let sheets = layout(&display_sizes, &settings)?;   // geometry only (previews)
let pdf = impose(&doc, &settings)?;                // the print-ready PDF
spool::submit(&pdf, &spool::Job { printer: None, copies: 2, ..Default::default() })?;
```

- **Size**: fit (to the sheet minus an 18 pt margin), actual size, shrink oversized, custom %;
  centred; auto orientation turns the sheet for landscape pages.
- **Multiple**: 2/4/6/9/16 (or any n) pages per sheet, horizontal/vertical (reversed) order,
  page borders, auto-rotation of pages that don't match the cell.
- **Cut and stack** (Multiple's page order): consecutive pages in each cell's pile,
  with aligned cut marks in the gutters. For example, 10 pages at 4 per sheet give
  `[1, 4, 7, 10]`, `[2, 5, 8, blank]`, `[3, 6, 9, blank]`. Print single-sided,
  keep sheets in output order, cut at the marks, then stack the cell piles from
  left to right, top to bottom. Blank cells can be discarded. Page ranges and
  Reverse pages are applied before imposition. Duplex imposition is not supported.
  Headless: `doc_print` with `layout: "multiple"`, `order: "cut-stack"` and
  `per_sheet: 4` (or any supported grid size), plus `path` or `printer`.
- **Booklet**: saddle-stitch imposition padded to a multiple of 4; both sides, front or back
  only; left or right binding.
- **Poster**: tile scale, overlap shared by neighbouring tiles, cut marks.
- **Comments & forms**: document, + markups, + stamps, or form fields only; annotations print
  only with their Print flag, drawn from their appearance streams (Algorithm 8.1).

Each source page becomes a Form XObject (its content wrapped in q/Q, plus the printable
annotations); sheets place them with a clip. The result is a fresh, unencrypted,
garbage-collected file (callers check the print permission).

`spool` talks to CUPS on macOS and Linux: printers come from `lpstat -p -d` (the queues) and
`lpstat -e` (driverless destinations no queue exists for; CUPS builds a temporary one when the
job arrives), jobs go to `lp` with copies, collation, duplex and monochrome options. The job is
piped to `lp` on stdin and never written to a temp file, where another local user could read or
swap it. On Windows everything happens in-process, with no PowerShell or other program started,
so printing also works where App Control for Business or AppLocker lock scripts down (#1107):
the printers come from `EnumPrinters` (local queues and the user's connections) with the default
from `GetDefaultPrinter`, and whether each driver takes two-sided settings from its
PrintCapabilities (the Print dialog says when it doesn't, #1154). The job is drawn by the in-box
`Windows.Data.Pdf` with Direct2D into
the XPS print API, as vectors, through the `winprint` crate. Copies, collation, two-sided, colour,
the sheet's paper and orientation and "no scaling" go in a PrintTicket that the driver validates
first; when it has no such paper, or would change the paper, orientation or scaling, nothing is
sent. A job has one ticket, so sheets that Auto orientation turned for landscape pages are turned
back with `/Rotate` and print in the same job. The job file exists for the seconds the spooler
reads it, in a private temporary folder. Pages are drawn by Windows' PDF engine, not PdfCraft's
renderer, so an unusual file can print slightly differently from how it looks on screen. Other
platforms report that printing to a printer isn't available yet; the print-ready PDF can always
be saved.

Not yet: the web spooler, print as image, poster labels, PostScript output, colour conversion for
grayscale.

## Architectural sheets and scale

The Paper menu and `doc_print` share Letter, Legal, Tabloid, A3/A4/A5 plus
ARCH A/B/C/D/E1/E, ANSI C/D/E and ISO A0/A1/A2. ARCH D is 24 × 36 inches;
ANSI D is 22 × 34 inches. Dimensions follow the [HP DesignJet paper-size table](https://support.hp.com/hr-en/document/bpp01676).
Automation accepts short names such as `"ARCH D"` (case-insensitive) or the full menu label.
For example, `doc_print` with `paper: "ARCH D"`, `layout: "actual"`,
`orientation: "auto"` and `path: "plan.pdf"` writes a print-ready PDF at 100%.

**Match page at 100%** uses the currently viewed document page to choose a standard
sheet (either orientation, within 0.5 pt for rounded metric dimensions), Actual size
and Auto orientation. It leaves the selected page range intact. It is disabled
for a custom page size with no matching preset. One preset applies to the whole job;
this is not per-page paper-source selection.

**PDF scale** describes the source-to-sheet transform in the previewed print-ready
PDF, not measured physical output. Fit reserves an 18 pt margin and can reduce even
a matching page. The current Windows backend still rasterises and can substitute
paper/shrink a job if its driver lacks the requested size. These controls do not
add #599's strict PrintTicket validation or vector spooler. Verify the driver's
paper/scaling settings and physical calibration before relying on drawing scale.
