Browser page shortcuts and organizer actions could independently consume the same clipboard gesture. This ports the existing *craft command-layer Copy/Cut/Paste routing and cross-frame gesture matching while preserving focused text and form controls. Pages keep their internal clipboard semantics.

Browser checkpoints restore ordered PDF tabs, active document, PDF snapshots and both history stacks, dirty status, per-document current page, scroll offset, zoom, fit, page layout, continuous-page mode, rotation and navigation history. The rebase preserves upstream's newer page-layout and camera actions.

Addresses the browser session-restoration portion of #418 and #1178. Desktop persistence and independently reopening a document after explicitly closing it are not implemented here, so neither report is automatically closed.

Validation so far: nine standalone storage tests passed for this app at the root URL; the common harness extracting the actual rebased production clipboard routers/PNG decoder passed eight tests. All four rebased applications passed local WebAssembly checks. Native engine history round-trip tests are still running and are not yet claimed passed. No GitHub Actions build was requested.

This is a draft port of existing code, rebased onto upstream main as captured by the fork. Launcher, shared fonts, branding removals and hosting changes are excluded. See docs/browser-session-recovery.md for storage limits, migration and testing details. The JavaScript IndexedDB/lifecycle coordinator is an explicit architecture proposal for review against the Rust-only repository preference.

Pending before merge: repository-wide quality checks and real Chromium/Firefox/WebKit clipboard permission, menu/keyboard, text focus, stale destination, refresh/return, duplicated-tab and large-history review. Compilation and fake-DOM storage tests do not establish complete browser behavior. AI assistance was used to port and review the changes.
