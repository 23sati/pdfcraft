#!/usr/bin/env bash
# Fork only (docs/fork.md): refresh the sha256 of the UI translation catalogs in ATTRIBUTION.toml.
# The fork adds strings to these catalogs, so their hashes differ from upstream's; fork-sync.yml
# runs this after a merge where only those hash lines conflicted.
set -euo pipefail
cd "$(dirname "$0")/../.."
for f in crates/ui-egui/src/i18n/*.tsv; do
  h="$(sha256sum "$f" | cut -d' ' -f1)"
  awk -v path="$f" -v hash="$h" '
    /^\[\[asset\]\]/ { here = 0 }
    $0 == "path = \"" path "\"" { here = 1 }
    here && /^sha256 = / { $0 = "sha256 = \"" hash "\"" }
    { print }
  ' ATTRIBUTION.toml >ATTRIBUTION.toml.new
  mv ATTRIBUTION.toml.new ATTRIBUTION.toml
done
