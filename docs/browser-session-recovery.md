# Browser session recovery

Status: draft implementation; browser coverage and large-history performance remain under review.

## Architecture

The browser host starts recovery without blocking PdfCraft's event loop. `browser_store.rs`
uses `web-sys` IndexedDB requests and transactions, with Rust futures and bounded timeouts.
`browser_session.rs` owns lifecycle and egui recovery controls. There is no handwritten
JavaScript, npm test dependency, DOM stylesheet or external coordinator script.

Each page load allocates a fresh random writer id. A reload reads the last committed id from
sessionStorage; a later visit offers the latest committed snapshot. Duplicated tabs fork the
snapshot into independent writers. The document snapshot, preceding snapshot and latest
pointer commit in one transaction. Only transaction completion marks a save successful.
Quota, blocked storage and timeout failures leave editing available and show Retry.
The UI does not overwrite an already-open document to restore a session.

Checkpoints retain ordered documents, dirty flags, both history stacks, active tab, page,
zoom, layout, rotation, scroll and navigation history. GPU resources and unfinished gestures
are not persisted. Capture waits for 1.5 seconds of inactivity and writes at most every two
seconds. Hard termination can lose changes after the last committed snapshot. Full-history
serialization is bounded to 512 MiB and can still be expensive. History is never silently
truncated. Older session ids are retained; storage may fill until site data is cleared.

## Passwords and compatibility

Schema 2 contains credential references and descriptive prompts, not document passwords.
This applies to current document credentials and all undo/redo states, including owner
passwords and passwords changed during editing. Restoring a protected session prompts for
each distinct nonempty credential. Passwords remain in memory only. A wrong or missing
password fails restoration atomically. PDF snapshots retain their PDF encryption; recovery
does not add an independent encryption layer to otherwise unencrypted documents or metadata.
The internal `Keys` type is deliberately not serializable.

The former draft's schema 1 is rejected. Its `pdfcraft-web-sessions` database is not read,
migrated or silently deleted. Anyone who ran that draft should export important documents
and then clear its legacy site data because it may contain plaintext passwords. The new
store is `pdfcraft-rust-sessions-v2`. Starting fresh preserves existing recovery copies.
Clearing browser data or browser eviction removes recovery.

## Controls and automation

The egui Session recovery panel offers restore/start fresh, masked password entry, retry,
recovery-copy import/export and a user-initiated persistent-storage request. Importing or
restoring requires closing current documents first. A downloaded recovery copy contains no
passwords but may contain confidential document content and editing history.

Headless tools: `session_capture {path}`, `session_inspect {path}` (password prompts), and
`session_restore {path, passwords?}`. Restore requires an empty session and validates embedded
document paths against `--root`. Capture writes atomically and does not mark documents saved.
These tools do not implement browser storage in native builds.

## Validation

Engine regressions cover tab/history round trips, localizable labels, changed document
passwords, owner passwords in redo-only states, no raw credentials in checkpoint bytes,
and atomic failure with existing work. An automation regression covers recovery and root
confinement. Browser IndexedDB and UI checks are tracked in the PR validation record.

Revision history: 2026-10-11 — replace the draft JavaScript coordinator with Rust and
replace stored credentials with password references; split clipboard routing into its own PR.
