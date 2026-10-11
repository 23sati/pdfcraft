Browser recovery is now Rust-only: web-sys IndexedDB, Rust futures and egui controls. Handwritten JavaScript, inline_js, session.css and npm test scaffolding were removed. Recovery readiness never gates the app's logic loop.

Schema 2 stores credential references instead of document passwords, including owner credentials and earlier undo/redo states. Restore prompts for required credentials and fails atomically on wrong or missing passwords. Old draft schema-1 stores are left untouched and rejected; see docs/browser-session-recovery.md for cleanup guidance.

Every page load has an independent writer id. Reloads recover the last committed snapshot; later visits offer restoration. Snapshot, previous snapshot and latest pointer commit atomically. The session includes document tabs, both history stacks, dirty state and view state. Headless capture/inspect/restore tools respect --root and refuse to replace open work.

The Organize clipboard changes have been removed and split into a separate PR. Addresses the browser portion of #418 and #1178 without claiming desktop recovery or independent reopening of explicitly closed documents.

Validation and remaining limitations are recorded in the PR conversation. The PR remains a draft pending real-browser and large-history review. AI assistance was used for implementation and review.
