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
from `GetDefaultPrinter`, and the job is drawn by the in-box `Windows.Data.Pdf` with Direct2D into
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
