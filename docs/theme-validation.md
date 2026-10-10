# Theme foundation validation

Validated on macOS on 2026-10-10, on `feature/extensible-themes`, based on upstream
main `68e91d4815ed43212895589411aa3d21b2a84bc3`. The Windows PR #616 checkout
remains unchanged at `2a884c911205332160808e05b326d6862a78a711`.

## Completed checks

- `cargo test --workspace --no-fail-fast`: 1,765 passed, zero failed, six ignored.
  The final run used native loopback/GPU access; the sandbox cannot provide those
  facilities required by existing control-channel and screenshot tests.
- After the final selector adjustment, all eight theme integration tests and 23
  control integration tests passed again, followed by Clippy and native builds.
- Theme tests cover all six palettes, stable IDs, token completeness, serialization,
  legacy settings, unknown/malformed/future data, System appearance changes,
  live switching with document state retained, and custom-library operations.
- Contrast checks cover critical text, links, feedback, selection, accent-button
  foregrounds and focus. A 1,024-entry library test verifies no arbitrary count cap.
- `cargo fmt --all -- --check`, workspace Clippy with warnings denied, native desktop
  and CLI builds, `cargo xtask assets`, and `cargo xtask layers` passed.
- Headless visual validation generated 32 home/document/menu/dialog screenshots
  covering Light, Dark and all six presets. Representative images of all six new
  presets were inspected. PDF paper remained white. Images are local validation
  artifacts only; system-font loading was disabled.

## Environment and upstream limits

- `cargo xtask wasm` was attempted but cannot compile: this installation lacks the
  `wasm32-unknown-unknown` Rust standard library and has no rustup installation.
  Browser compilation and browser persistence still need CI validation.
- Windows, Linux and FreeBSD builds were not run on this macOS host.
- `cargo xtask parity` fails on duplicate `evidence`/`notes` keys in the existing
  `form.field-list` entry. The same invalid keys exist at the upstream base commit;
  this branch leaves that unrelated defect untouched.
- `cargo xtask deny` reports skipped because cargo-deny is not installed.
- Optional craft-fonts input was unavailable; the full suite retains its existing
  optional-input skips. No fonts or dependencies were added.

Architecture, persistence details and the separate editor proposal are in
[themes.md](themes.md). The visual editor and import/export UI are future work.
