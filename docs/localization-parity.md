# Localization parity

> **Last reviewed:** 2026-10-11 · **Last updated:** 2026-10-11 · **Change:** major (five complete catalogs and embedded script coverage) · **Target:** Adobe Acrobat Pro (Acrobat DC, continuous track 26.002.21931, macOS)

Per-language status of PdfCraft's interface against Acrobat Pro's. How translations work and how to
add a language: [localization.md](localization.md). Summary in [ROADMAP.md](../ROADMAP.md).

## How this was measured

- **Strings translated** is measured: the catalogs in `crates/ui-egui/src/i18n/<code>.tsv`, counted
  as distinct `(context, English)` keys with a non-empty translation. The denominator is the union
  of keys across all catalogs, **2,229**. The five new catalogs also cover every interface literal
  collected by the repository's `ui_literals` test and every registered command/menu label.
  English is built in. Catalog completeness does not imply translated engine diagnostics or help.
- **Not counted:** error details from the engine and the operating system, which are shown in
  English inside a translated frame; there is no translated user documentation or help yet (Acrobat
  ships localized help).
- **Script support** comes from [localization.md](localization.md), the vendored winit IME patches
  (`vendor/README.md`) and user reports.
- **Native review**: "community" means a native speaker contributed or corrected the catalog in a
  pull request or issue; "no" means no record of review. No catalog has had a formal review.
- **Acrobat** ships 24 interface localizations on macOS (`.lproj`: cs, da, de, en, es, fi, fr, hu,
  it, ja, ko, nl, no, pl, pt, ru, sv, tr, uk, zh_CN, zh_TW, plus en_AE, en_IL, fr_MA) and Middle
  East / North Africa editions (MEA Arabic, MEH Hebrew, NAF French; `Resources/Sequences/`).

## The twelve key languages

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support | Native review | Status | Acrobat ships it | To `full` (h) |
|---|---|---|---|---|---|---|---|---|
| English | `en` | built in (100%) | all; README and docs | Latin | yes | **full** | yes | 0 |
| Simplified Chinese | `zh-hans` | 2,155 (96.7%) | dialogs and tooltips through `tl!`; no help | CJK text and IME work, but **release builds have no Hans face**: many characters show as boxes (#826, #688, #728, #758; Flatpak #689) | no | partial | yes | 6–12 |
| Spanish | `es` | 2,112 (94.8%) | dialogs and tooltips; no help | Latin | no | partial | yes | 2–4 |
| Hindi | `hi` | 2,229 (100.0%) | dialogs and tooltips; no help | Embedded Noto Sans Devanagari; conjunct shaping and every catalog character tested without system fonts | no | partial | no | not re-estimated |
| Arabic | `ar` | 2,208 (99.1%) | dialogs and tooltips; no help | Letters joined and lines reordered at load, but **layout not mirrored**, wrapped lines break at the wrong end, typed text runs left to right (`localization.md` §Arabic) | no | partial | yes (Middle East edition) | 25–45 |
| French | `fr` | 2,195 (98.5%) | dialogs and tooltips; no help | Latin | no | partial | yes | 1–2 |
| Portuguese (Brazil) | `pt-br` | 2,197 (98.6%) | dialogs and tooltips; no help | Latin | community (#288) | partial | yes | 1–2 |
| Indonesian | `id` | 2,229 (100.0%) | dialogs and tooltips; no help | Bundled Latin faces; every catalog character tested | no | partial | no | not re-estimated |
| Japanese | `ja` | 2,190 (98.3%) | dialogs and tooltips; no help | BIZ UDPGothic from craft-fonts; IME patched on macOS, Windows, X11, Wayland; vertical text renders but can't be edited | no | partial | yes | 4–8 |
| German | `de` | 2,195 (98.5%) | dialogs and tooltips; no help | Latin | community, in progress (#828: terminology, clipping) | partial | yes | 2–4 |
| Korean | `ko` | 2,229 (100.0%) | dialogs and tooltips; no help | Embedded NanumGothic; every catalog character tested; existing IME patches unchanged | no | partial | yes | not re-estimated |
| Vietnamese | `vi` | 0 | none | Latin with stacked diacritics; bundled Inter covers them (not tested) | no | none | no | 3–5 |

**The previous overall localization score and remaining-hour estimate have not been re-estimated.**
The five new catalogs cover the measured key union; layout, translated help, native review and
script-specific limitations remain separate from string counts.

No language is `full` except English: every catalog leaves engine error details in English, and
none has translated help.

## Other shipped languages

| Language | Code | UI strings translated | Status | Acrobat ships it |
|---|---|---|---|---|
| Traditional Chinese | `zh-hant` | 2,083 (93.4%) | partial (same Hans/Hant face caveat) | yes |
| Russian | `ru` | 2,110 (94.7%) | partial | yes |
| Bulgarian | `bg` | 2,161 (96.9%) | partial | no |
| Czech | `cs` | 167 (7.5%) | menus only | yes |
| Telugu | `te` | 2,124 (95.3%) | partial (the release-pinned craft-fonts has no Telugu face yet) | no |
| Hungarian | `hu` | 2,193 (98.4%) | partial | yes |
| Ukrainian | `uk` | 2,207 (99.0%) | partial | yes |
| Italian | `it` | 2,195 (98.5%) | partial | yes |
| Polish | `pl` | 2,186 (98.1%) | partial | yes |
| Bengali | `bn` | 2,229 (100.0%) | partial; embedded Noto Sans Bengali, conjunct shaping and every catalog character tested; no help/native review | no |
| Urdu | `ur` | 2,229 (100.0%) | partial; embedded Noto Sans Arabic joins Urdu letters and reorders labels; Arabic's remaining layout/wrapping/input limitations also apply | no |

**11 other languages** are registered (21 catalogs plus English, 22 interface languages in Preferences).
Acrobat languages PdfCraft lacks: Danish, Dutch, Finnish, Norwegian, Swedish, Turkish,
Hebrew.

The new script-font input is pinned to [craft-fonts #19](https://github.com/storytold/craft-fonts/pull/19).
Turkish [#1057](https://github.com/storytold/pdfcraft/pull/1057) and Chinese font delivery
[#879](https://github.com/storytold/pdfcraft/pull/879) / [craft-fonts #17](https://github.com/storytold/craft-fonts/pull/17)
are separate existing contributions, not included in these counts or claimed as fixed here.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-11 | major | Added Hindi, Bengali, Urdu, Indonesian and Korean; re-counted all 22 registered languages against 2,204 keys; documented embedded script tests and remaining RTL/help/review limits. No new aggregate readiness estimate |
| 2026-10-10 | major | Created. Catalog counts measured from `crates/ui-egui/src/i18n/*.tsv` against the 2,143-key union; Acrobat's localizations from its bundle's `.lproj` and Sequences folder names |
