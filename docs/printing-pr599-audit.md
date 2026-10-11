# Windows printing: #599 compared with upstream

Audited 2026-10-11 against upstream `afb586e2497e7754c8cefc031d524432d14124f5`.
This is a code/history comparison; physical printer behavior was not re-tested here.

## Revisions and provenance

- Original [#599](https://github.com/storytold/pdfcraft/pull/599):
  `2f20c8c361b266c27dd7e34c231cf882e41bafc8` plus lint update
  `0f65eacf6642c191a8842e14cf8d15e5d9896c5e`, by @durtyrobbin with Codex assistance.
- [#847](https://github.com/storytold/pdfcraft/pull/847), by @Mawfyy: merged as
  `33c14e07cfd742a82bf03baa6b103c01008fa68c`.
- [#1007](https://github.com/storytold/pdfcraft/pull/1007), by @echelon: merged as
  `bbbe60015c571ebfc39708ee07777b89e267122a`.
- Comparison endpoint: [`afb586e`](https://github.com/storytold/pdfcraft/commit/afb586e).

Neither #599 commit is an ancestor of the comparison endpoint. `git cherry`
marks both with `+`, so neither has a whole-commit patch equivalent there.
Inspection of the printing diffs finds different implementations: #599 uses
`winprint`, PrintCapabilities and validated PrintTickets; #847 uses a PowerShell
script with `System.Drawing.Printing`; #1007 revises that script and the UI workers.
An added-line comparison (trimmed nontrivial lines, preserving order) finds only
single-line matches with #847 and a two-line repaint idiom with #1007. This is
limited evidence against direct textual reuse in the examined changes, not proof
that ideas, review feedback or rewritten fragments were never used indirectly.

#847 explicitly references #452 for the CIM enumeration approach. #1007 explicitly
follows #847. The examined PR bodies, comments and commits do not establish that
#599 supplied their implementation. Similar requirements and shared pre-existing
print imposition/UI code cannot establish authorship of a later implementation.
The maintainer's [closure comment](https://github.com/storytold/pdfcraft/pull/599#issuecomment-6110689298)
says Windows printing was superseded and welcomes focused missing pieces; it does
not say #599 was inferior, or prove that no part influenced subsequent work.

## Feature comparison

| Feature in #599 | Upstream at the comparison endpoint | Assessment / action |
|---|---|---|
| Windows queue names and default | #847: CIM via PowerShell | Equivalent user capability, different implementation. #599 has no PowerShell dependency. |
| Discovery error reporting | #599 `try_printers` returns an error; upstream returns an empty list on command failure | Missing diagnostic distinction; not ported in this paper-controls change. |
| Vector-capable Windows PDF submission | #599: Windows.Data.Pdf/Direct2D/XPS through safe `winprint`; upstream: Windows.Data.Pdf bitmap + .NET PrintDocument | Not equivalent fidelity. Raster-only upstream loses vector/text structure; #599's vector result is independently reported, not newly tested here. |
| Copies/collation/duplex/grayscale requests | #847 passes .NET job settings, explicitly including Simplex | Broad capability overlap; driver-private options and exact output are not proven equivalent. Grayscale is a driver request in both. |
| Background submission and failure notice | #1007 workers and result channels | Equivalent core behavior, implemented differently. Upstream applies it beyond Windows. |
| Reject another pending submission | #599 has a pending-job guard; upstream keeps a vector of concurrent jobs | Missing guard relative to #599; concurrent jobs may be intentional, not automatically a regression. |
| Background queue discovery | #1007 adds asynchronous discovery and protects a user's late selection | Upstream improvement; #599 discovery was synchronous. Retained. |
| Temp file containment/cleanup | Both create temporary PDFs and clean after submission; upstream uses unpredictable exclusive creation | Equivalent purpose, separate implementation. No crash-proof deletion claim. |
| ARCH A/B/C/D/E1/E, ANSI C/D/E, A0/A1/A2 | Upstream catalog still has only Letter/Legal/Tabloid/A3/A4/A5 | Missing. Restored in this focused follow-up, including CLI/MCP names. |
| Match current page at 100% | Absent | Restored: choose matching preset, Actual size, Auto orientation; preserve range. |
| Effective preview scale | Absent | Restored as **PDF scale**, with explicit driver limitation. Also shows dimensions for the selected preview sheet. |
| Exact driver media / merged-ticket validation | #599 rejects changed dimensions, orientation or extra scaling; #1007 picks nearest advertised media or retains driver default and may shrink | Strict media/scaling validation is missing. This follow-up does not claim or add it. |
| Mixed page orientations in one Windows job | #599 rejects mixed size/orientation; #1007 queries settings per sheet | Upstream improvement in code. Physical mixed-job calibration unverified here. |
| No child process in list/spool path | #599 native wrapper; #847/#1007 PowerShell (hidden console after #1007) | Hiding a console is not process elimination. Managed-policy compatibility remains different (also independently reported in #1107). |
| Unix stdin spooling / Save as PDF | Shared existing capabilities retained by both | Not new features attributable to #599. Upstream also gained driverless CUPS discovery (#658) and Properties (#482). |
| Web physical printing | Neither adds a browser spooler | No change; browser Save as PDF remains separate. |

## Evidence limits and follow-up scope

Code references: `crates/print/src/{lib.rs,spool.rs,windows_print.ps1}` and
`crates/ui-egui/src/print_ui.rs` at the endpoint, versus
`crates/print/src/spool/windows.rs` and the corresponding UI files at `0f65eac`.

Independent reports on #599 include
[print-shop driver/file-port testing](https://github.com/storytold/pdfcraft/pull/599#issuecomment-6088828583)
and [an MSP's physical Xerox test](https://github.com/storytold/pdfcraft/pull/599#issuecomment-6100121484).
They report strengths of #599 and driver-specific limitations; they are third-party
reports, not results of this audit. Their comparisons to #421/#783 must not be
presented as tests of #847/#1007. In particular, #847 explicitly requests Simplex.

[Issue #1107](https://github.com/storytold/pdfcraft/issues/1107) reports a direct
comparison under WDAC script enforcement: main at `bbbe600` listed printers but
failed to print, while the `d4fa174` branch built on #599 printed under the same
policy. This is stronger evidence of a specific upstream limitation than a generic
PowerShell concern; it remains a third-party report, not a local rerun. No security
policy was changed during this audit.

The native approach also has costs: #599 adds `winprint` and its transitive build
dependencies, and restricts each Windows job to one sheet size/orientation. The MSP
report raises a dependency-license review concern. Neither approach is universally
superior; the raster and native paths both use the Windows PDF renderer, so neither
establishes identical rendering to PdfCraft for every input.

The new PR intentionally ports the missing paper catalog and associated controls,
with source-to-sheet geometry, saved-file/tool and GUI regressions. It retains the
merged backend. A native/vector backend, strict driver-media validation, discovery
errors and duplicate-job policy remain separate reviewable work; the original
#599 implementation/history is preserved as evidence and a possible starting point.

## Validation of this focused port

Windows release checks used `CARGO_PROFILE_RELEASE_LTO=false` and
`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`; no optional craft-fonts checkout.
Formatting, strict workspace/all-targets Clippy, assets, parity and all 30 library
WASM checks passed. The print crate passed 25 tests, the automation integration
suite passed 115, and the print-dialog suite passed 9. The live test that creates
a printer queue and submits a job was deliberately excluded.

The native CLI exported every new preset in both orientations. Independent
PyMuPDF 1.28.2 checks confirmed all 24 MediaBoxes (within 0.02 pt), and a synthetic
2 × 1 inch rectangle remained 144 × 72 pt under Actual size. Fit reduced that
rectangle to approximately 141.005 × 70.502 pt. The offscreen ARCH D dialog was
visually inspected: dimensions, matching control, Actual size and PDF scale are
visible and unclipped. No user documents or physical printers were used.

The initial full-workspace run was **not fully green**: 2,155 passed, 7 failed,
7 ignored. Three single-instance socket tests passed with a shorter E: temporary
path (all 8 single-instance tests then passed); three unchanged text-selection
tests passed when rerun serially. The remaining unchanged automation confinement
test ignores failure to create its symlink fixture, then expects the ordinary
`link-out/outside.pdf` path to behave as an escape link. This host denies symlink
creation with Windows error 1314. Its root-traversal checks and the new paper
export's root-boundary regression still ran; no actual outside-root write was
observed. The test was not weakened and host security settings were not changed.
Linux/macOS execution and physical driver calibration remain unverified.
