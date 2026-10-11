# Editing existing images and figures

In **Edit a PDF → Edit text and images**, click a figure to select it. Drag the selected
figure to move it, drag a corner to resize it proportionally, or press Delete to remove
that placement. The context menu also rotates or flips it. Changes support undo and
survive saving/reopening.

PDF figures are not always bitmap images. Imported diagrams often use a **Form XObject**,
which packages text, vector paths, and nested images into one object. PdfCraft selects
this object using its `BBox` and optional `Matrix` and edits it as a whole. Its original
content, resources, clipping box, and transparency settings remain intact. Moving or
deleting one use of a shared figure does not alter its other placements or pages.

**Replace Image** and **Save Image As** apply only to raster Image XObjects; these menu
items are disabled for grouped Form artwork. The tool does not ungroup Forms, edit their
individual internal elements, select arbitrary ungrouped vector paths, or edit inline
images (`BI`/`ID`/`EI`). A Form's bounding box may include whitespace.

For automation, `page_images` lists both kinds with `kind: "image"` or `kind: "form"`.
Forms have `pixels: [0, 0]`, since they have no single bitmap resolution. Use the returned
number with `image_edit` (move, rotate, flip_horizontal, flip_vertical, delete).
Coordinates are top-left-origin page points. `image_save` and `image_edit`'s replace action
return a clear error for Form artwork.

Selection frames follow the visible portion of artwork after page and path clipping. Clicking
editable text takes precedence over overlapping artwork, including labels inside an image's
visible rectangle. Hidden image placements have no selection frame. Rectangular clips are
exact; curves and complex paths use conservative bounds, so holes are not excluded from hit
testing. Moving or resizing an image changes its full placement while its existing clip remains
fixed; the preview shows that cropped result.

`page_images` retains `rect` as the full placement rectangle used by `image_edit` and adds
`visible_rect` for selection (null when fully clipped). `object_list` reports visible artwork
bounds while keeping the original source indexes. A selection frame does not create, remove
or enlarge the PDF's clipping path.
