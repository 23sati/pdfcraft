# Theme foundation and automation validation

Validated on macOS on 2026-10-11 on `feature/extensible-themes`. Original commits
`70e2846` and `af43d9a` are preserved. Upstream main
`798650e8a09607471dec48e1aa7949a4d37b9aec` was merged as `dc687c1`, then
`b8ddf71e30959dc8a5ca4b66c40b8fcaea68cf76` was fetched, reviewed and integrated
without rewriting existing commits. The latest merge retains Linux desktop
appearance tracking and adds the preset names to Polish. Closed PR #616 and its separate checkout remain untouched at
`2a884c911205332160808e05b326d6862a78a711`.

## Naming refinement (PR #1090)

Atelier, Editorial, Heritage, Executive, Sienna, and Paper & Ink are the display
names. The original IDs and all palette values remain unchanged. A before/after
CLI comparison verified every one of the 31 colors for all nine selections.
All 16 localization catalogs contain the six display-name keys exactly once.
The original Catppuccin/Solarized license notices and license files are unchanged.
Regression coverage checks the GUI selectors, CLI/MCP display metadata, serialized
IDs, and imported partial custom definitions resolving the old base IDs after a
restart, export and active-theme deletion.

Latest upstream `bbbe60015c571ebfc39708ee07777b89e267122a` was fetched and reviewed.
It changes version manifests and shared UI/icon integration, but not the theme
core. GitHub reports the PR as conflicting; merge preview identifies
`ATTRIBUTION.toml`, `Cargo.toml`, `crates/ui-egui/Cargo.toml`, and
`crates/ui-egui/src/icons.rs`. These conflicts already existed before this naming
commit. Upstream was not merged into this focused refinement because doing so
would introduce unrelated dependency and UI changes. The PR is not mergeable
until that integration is completed separately.

The final naming rerun passed formatting, workspace Clippy with warnings denied,
all 2,125 workspace tests (zero failures, six existing ignores), native desktop/CLI
builds, assets and layering. The 34-image visual test was regenerated and the menu
was inspected with the new labels. The parity failure and missing WASM/cargo-deny
limitations below remain unchanged.

## Passed

- `cargo test --workspace --no-fail-fast`: **2,125 passed, zero failed, six ignored**.
  This repeats and expands the original 1,765-test baseline after upstream integration.
  The six existing ignores concern network access, two optional corpus tests, a
  timing probe, a local signature diagnostic and a temporary macOS keychain test.
- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo build -p pdfcraft -p pdfcraft-cli` on native macOS.
- `cargo xtask assets`: 264 repository assets, six dependency assets, 22 build inputs.
- `cargo xtask layers`: 35 crates, no layering violations.
- Focused final reruns: all 21 shared-core/store tests, all nine theme integration
  tests, and the adjusted MCP structured-error regression. The core tests include
  refusal of a standalone export accidentally supplied as a store. A new unit
  regression verifies Linux desktop appearance fallback through the shared palette
  without overriding explicitly selected presets.

Native control-channel and screenshot tests need loopback/GPU access, so the full
workspace and visual runs used an execution environment with those facilities.

## Coverage

The shared core tests all six presets, stable IDs, all 31 token names and edits,
complete palette inspection, creation, cloning, patches, renaming, activation,
deleting the active theme, versioned import/export, duplicate names, ID collisions,
legacy preferences, unknown extensions and token data, unsupported schemas,
invalid colors, and preservation of exact translucent input values. Failed
operations leave the state unchanged. A 1,024-entry library test retains the
original guarantee of no arbitrary entry-count cap.

Store tests cover atomic replacement, invalid/malformed files, migration from
legacy app.ron without changing it, revisions, lock contention, concurrent
independent edits, symlink refusal, future schemas and failure-safe writes. GUI
integration verifies that a CLI edit survives a subsequent GUI selection and
restart, and that a malformed store is not replaced by a preference change.

Headless CLI tests run the complete create/inspect/update/rename/activate/clone/
export/delete/import workflow in separate processes, check all palette keys,
System/Light/Dark, malformed input, JSON errors and nonzero exits. MCP tests verify
normal and compact tool discovery/invocation, structured failures and registry
schemas. Existing hostile-input and root-confinement tests also cover the new tools.

Contrast tests check critical text and interaction states. Editing reports warnings
rather than changing user-selected colors; translucent roles report that effective
contrast depends on the surface below them.

Visual validation generated **34** local home/document/menu/dialog images covering
Light, Dark, all six presets, and a custom navy-and-cream theme. The custom menu was
inspected and selected through the existing GUI selector. PDF paper stayed white.
System-font loading was disabled; images and logs are ignored local artifacts.

## Remaining upstream and environment limits

- `cargo xtask parity` fails on a duplicate `evidence` key at line 5254 in
  `parity/acrobat-features.toml`. Parsing the same file from unmodified upstream
  `b8ddf71` reproduces the failure. The earlier `798650e` validation passed; this
  later upstream failure is not reported as a successful check.

- `cargo xtask wasm` was attempted and failed because this Rust installation lacks
  the `wasm32-unknown-unknown` standard library. Browser compilation/persistence
  still need CI validation. Native filesystem tools are explicitly gated out on
  WASM; browser hosts use the shared in-memory API.
- Windows, Linux and FreeBSD builds were not run on this macOS host.
- `cargo xtask deny` reported **SKIPPED** because cargo-deny is not installed; its
  zero wrapper exit code is not a successful dependency audit.
- Optional craft-fonts and corpus inputs were not installed. No fonts or new
  third-party dependency packages were added.

Natural-language generation and a graphical custom-theme editor remain future
work. Their integration plan, native storage concurrency protocol, browser limits
and import/export contract are documented in [themes.md](themes.md).
