# Editing existing text and source subsets

Click a text region in **Edit a PDF → Edit text and images**. One editor contains its
source rows and mixed styles. Embedded TrueType and OpenType/CFF faces are used in memory;
source font programs, colour, size and baseline anchors stay intact. A textless preview
prevents a second copy underneath. The frame shows the live extent as text grows or shrinks.

A subset font contains only some of the original face's glyphs. A new character cannot be
reconstructed from a glyph the PDF does not contain. The desktop automatically discovers
eligible installed `.ttf`/`.otf` faces in the background when a document opens. **Keep source
fonts** matches the subset's PostScript face name against these complete programs, including
new Chinese characters. Filename and family-only guesses never select a different weight or
italic face. Discovery and font bytes are cached: typing performs no filesystem scan. Preview
and Apply use the same bytes, and a completed preview map is reused between keystrokes.

The full matching face is embedded only when a text edit needs absent glyphs. Completion and
text are one atomic, undoable change; discovery and previews do not dirty the document.
**Load font…** and **Installed fonts…** remain available for explicitly changing a face.
Fonts with restricted embedding, unsupported collections, excluded families or no
open-licence declaration are refused. Interface fallback fonts are never exported.

The full selected program, glyph mappings, widths and Unicode text are embedded in the PDF.
Saving and reopening therefore needs no locally installed font. Original subset bytes are
preserved. Optional matching faces from `CRAFT_FONTS_DIR` can also complete source subsets;
this works only when that build input actually contains the full face. This change does not
add a font binary or change the release font pin. Fonts shared by releases belong in
`storytold/craft-fonts` under the repository's asset rules.

Select text within the editor before changing size, colour, font, underline or spacing to
format that selection. Automation supports up to 1024 disjoint UTF-8 byte ranges with the
same properties; invalid or overlapping ranges fail atomically. Untouched source formatting
survives. The existing single-face OpenType shaping path writes ToUnicode/ActualText. Main's
Hebrew single-style path and refusal of partial Hebrew formatting stay unchanged. Mixed
Arabic bidi extraction still has a known optional-test failure; do not claim RTL parity.

Headless document opening also prepares a font catalogue. With `--root`, discovery is limited
to that root; it never reads system fonts outside it. `text_font_refresh` refreshes the
catalogue or selects confined directories for automatically matching future edits. Neither
`text_font_import` nor a resource selection is required for matching subset completion.
For an explicit face change, use `text_font_files` → `text_font_import` → `text_fonts` and pass
a resource to `text_edit.font_resource`. `text_edit.ranges` holds selected range formatting.
`text_edit_layout` measures a draft without applying it. All file inputs respect root
confinement. The engine API is `Catalogue::discover` → `Session::set_text_font_catalogue`;
callers control discovery roots rather than enabling ambient filesystem access implicitly.

Explicit marked paragraph/heading scopes prevent unrelated tagged regions from merging.
Compatible geometry groups untagged content, including runs split across content streams
or equivalent graphics transforms. Geometry cannot infer author intent in every untagged
PDF. Semantic structure-tree preservation, legacy Type 1/Type 3 inline font preview,
font collections/variable-font instances, mixed-font/style contextual runs and complete rotated
layout fidelity still need broader coverage. Do not interpret the passing real-file checks
as a guarantee for every PDF.

Private acceptance tests are opt-in and keep inputs unmodified. They exercise actual Chinese
keyboard insertion/deletion on all four pages with the matching face discovered in a private
font directory and no Load font edit, source-face/baseline checks and identical
in-memory renders before and after saving/reopening. Synthetic tests cover full TrueType
mapping, OpenType/CFF, repeated edits, disjoint styles,
invalid ranges, font selection, automatic face matching, wrong-style rejection, one-step
undo, cached file removal and root confinement. Reopened edits also insert a previously unused
character with no installed font file. CLI acceptance repeats insertion/deletion
and two saves/reopens on four pages; independent PDFium checks extraction and rendering.
No private document, font or screenshot
is included in the repository.
