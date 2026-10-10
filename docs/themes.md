# Extensible application themes

## Assessment and scope

Based on upstream main `68e91d4815ed43212895589411aa3d21b2a84bc3` (2026-10-09).
This branch is independent of Windows single-instance PR #616.

Upstream PR #212 (`42078a8`) unified the toolbar and View menu around
ThemePreference and a single setter, preserving System as a user preference
separate from its resolved Light/Dark appearance. Earlier #76 raised text
contrast. The existing theme module provides semantic Tokens and maps them into
egui Visuals; most custom chrome already uses them. Feedback colors and some
widget defaults remained scattered. Font, spacing and layout changes are out of scope.

The existing settings are a JSON object stored by eframe, including on the web.
The `theme` property contains legacy strings System, Light and Dark. The new
presets add stable lowercase IDs without changing those legacy encodings or the
default. Unknown selections retain the current preference. System still resolves
OS appearance each frame. Explicit palettes do not follow OS changes.

## Architecture

Palette is the typed, resolved set of supported application colors; Tokens is a
compatibility alias for its existing consumers. Light/dark kind is an egui hint,
not theme identity. Built-ins and custom definitions resolve into the same type.
The palette registry supplies stable token keys for serialization and the future
editor. Hexadecimal color values are portable and human-readable. Layout metrics
are not color tokens.

The versioned theme library is separate from the selected preference. Custom
identities are stable IDs, not names; renaming never breaks references. Built-ins
remain immutable; duplication creates a new definition with its own ID and base
reference. Unknown fields and token data are retained, and unsupported definitions
fall back for display. Missing/invalid tokens use the base palette. No theme-count
limit is imposed. Existing serde/egui facilities suffice; no theme crate is needed.

Application surfaces, controls, focus, selection, text, feedback and shadows are
covered. PDF paper, pixels, annotation colors, redaction, signature ink and document
editing overlays retain their document semantics. Native OS file dialogs remain
styled by the OS. System, Light and Dark retain their original visual mapping.

Settings inherit eframe's platform persistence and storage limits. A future editor
must report storage errors and export backups; an unbounded collection does not
promise infinite storage or durability beyond the existing backend.

## Schema and token contract

The current custom record has `schema_version`, UUID-shaped `id`, `name`, `kind`,
`base_theme_id`, `palette`, `metadata`, optional `legacy_defaults`, and preserved
unknown fields. For example (partial palettes are valid):

```json
{
  "schema_version": 1,
  "id": "01234567-89ab-cdef-0123-456789abcdef",
  "name": "My Ocean",
  "kind": "Dark",
  "base_theme_id": "ocean",
  "palette": { "accent": "#67cbe7", "window_shadow": "#0000006e" },
  "metadata": {}
}
```

The containing `theme_library` has a schema version and `themes` array. Selection
stays under `theme`: existing strings `System`, `Light`, `Dark`; lowercase preset
IDs; or `{ "Custom": "01234567-89ab-cdef-0123-456789abcdef" }`. Control options use
`custom:<uuid>`. Display names are never identity keys. UUID generation belongs to
the future creation UI; the core validates and accepts caller-supplied IDs.

`ColorToken::ALL` is the exhaustive registry of 31 colors. Adding a Palette color
without registering it fails compilation. Groups cover surfaces, text, controls,
accents, feedback and shadows. Panel is shared by sidebars and tool panels; card
is shared by dialogs and popup surfaces. Chrome is the toolbar/mode bar; pasteboard
is the application surface behind PDF pages. AccentText is used for links and
accent-button hover; OnAccent supplies their readable foreground. TextFaint is for
hints/captions; egui also applies its disabled-opacity treatment. Focus supplies
keyboard focus and text-cursor strokes. Every token has typed get/set access and
hex serialization; no raw PDF content color is registered.

`legacy_defaults` preserves the original Light/Dark egui defaults and their
historical feedback variations in an untouched copy. Changing a token overrides
that role everywhere, including these legacy consumers. New presets use the full
palette mapping. Resetting a copied legacy token restores its original treatment.

Base references currently resolve concrete built-ins, never recursive custom-theme
inheritance. Definitions store their own values; a future duplicate of a custom
theme should snapshot its resolved colors and keep its original built-in base.
Unknown bases fall back to the record's light/dark kind. Unsupported versions and
invalid entries remain stored but display safe fallbacks, with load diagnostics.
Future migrations should be explicit version-to-version transforms of the retained
raw envelope. The core already supports insert, replace, rename and remove; editor
Save must persist the changed library and surface any backend storage error.

Forest, Ocean, Sunset and Paper & Ink use all 14 existing translation catalogs;
Catppuccin and Solarized retain their palette names. Both external inspirations are
MIT-licensed, with notices in NOTICE and full licenses in `docs/licenses/`.

## Separate editor PR

1. Add a theme library with virtualized rows, built-in and custom sections, and no
   arbitrary entry cap. Offer Create, Duplicate, Rename and Delete. Generate a
   random stable ID on creation using platform-supported entropy; never derive it
   from a label. Deleting the active custom theme selects its base or Light.
2. Open a dedicated editor with collapsible Surfaces, Text, Controls, Borders,
   Feedback and Accents groups generated from the token registry. Each row has a
   color picker, validated hex input, sample and Restore default action. Include
   whole-palette Reset to base and a small preview of menus, inputs and status text.
3. Keep a draft separate from the saved library. Live preview changes only the
   effective palette. Apply keeps a session preview; Save atomically updates the
   definition and selection; Cancel restores the pre-edit selection and palette.
   Closing or switching with dirty drafts offers Save, Discard or Continue editing.
4. Show contrast warnings for text/surface combinations (4.5:1), focus and important
   non-text affordances (3:1). Invalid syntax blocks Save; accessibility warnings
   are explicit and explain the affected roles. Do not silently rewrite colors.
5. Import/export the versioned JSON definition, preserving unknown metadata.
   Validate IDs, names, versions and colors. Imported ID collisions offer a new
   copy or explicit replacement. Unsupported schema versions are preserved but
   read-only; migrate only with an explicit versioned transformation and backup.
6. Add restart, storage-failure, Cancel/Apply/Save, duplicate-name, collision,
   corrupt-token, schema-migration and large-library tests. Exercise native and
   browser persistence, keyboard navigation and every token's live update.

The editor should use these models rather than introduce another appearance
preference, palette format or independent style pipeline.
