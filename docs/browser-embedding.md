# Browser embedding and graphics recovery

The browser build exposes `window.pdfcraft` after the `pdfcraft:ready` window event.
This API is implemented in Rust through wasm-bindgen; no handwritten JavaScript ships with the app.

## Direct API

Call `await pdfcraft.open(bytes, name)` with an `ArrayBuffer` or `Uint8Array` (1 byte to 128 MiB),
and a nonempty document name of at most 200 characters. It accepts the document immediately;
encrypted documents may still ask for a password in the editor. Invalid inputs reject the Promise.

`await pdfcraft.save()` returns `{name, bytes}`, where `bytes` is an `ArrayBuffer` containing
the active PDF. Pending form/Fill & Sign typing is committed using the same path as Save.
An export does **not** clear dirty status: the host still has to persist those bytes. Failed
commits reject the Promise and retain the draft. Unsupported unfinished content cannot be exported.

`pdfcraft.isDirty()` returns whether any tab has unsaved work. The window's `pdfcraft:dirty`
CustomEvent carries that Boolean in `event.detail`; it fires when dirty status changes.
The existing engine/CLI save APIs remain the headless document-processing interface.

## Parent messages

Embed with `?host=parent`. By default only a parent on the app's own origin is accepted.
For cross-origin deployments explicitly add `&parentOrigin=https%3A%2F%2Fhost.example` to the
iframe URL. This is exactly one trusted HTTP(S) origin, including its port when nondefault;
paths, credentials, wildcards and opaque origins are rejected. It grants the configured parent
access to the documents in this frame, so configure it only in a trusted embedding page.

Send messages to the iframe's exact origin from its actual parent window:

- `{type: "pdfcraft:open", requestId: "1", name: "review.pdf", bytes: arrayBuffer}`
- `{type: "pdfcraft:save", requestId: "2"}`
- `{type: "pdfcraft:isDirty", requestId: "3"}`

Replies are `{type: "pdfcraft:result", requestId, ok: true, result}` or
`{type: "pdfcraft:result", requestId, ok: false, error}`. The save result is the direct API's
`{name, bytes}` shape. Correlate replies by requestId, verify the iframe source and exact origin,
and impose a host-side timeout. Unknown commands and requests from other windows/origins are ignored.
Request IDs must be nonempty strings of at most 200 UTF-8 bytes. Responses use the exact parent
origin, never `*`. The host's change listener remains the existing `pdfcraft:dirty` message.

Legacy messages without a requestId retain their existing open/menu-save behavior. Direct API
and request-response exports intentionally do not acknowledge durable host storage; legacy menu
saves retain their historical behavior. The API does not supply optional PNG page export.

## Graphics loss (#168)

On `webglcontextlost`, a DOM recovery screen appears outside the failed canvas. Editing and
application frame logic stop, and API opens reject. The app remains owned outside the renderer,
so failure of the graphics runner cannot release its documents. Each open tab has an explicit
Download button, which uses the same export path as the API. Names are assigned with textContent,
not HTML. Download failures remain visible and do not trigger a reload.

Reload is always a deliberate user action. It discards the in-memory session and undo history;
the screen tells users to download every PDF first, and the normal unsaved-work browser warning
remains installed. This is actionable recovery, not automatic GPU/context reconstruction or
persistent undo recovery. Drafts that cannot be committed require resolving their normal warning
before export; do not reload if a recovery download fails. Hardware-induced loss and browser
permission behavior still require manual browser verification.

## Chinese interface fonts (#498)

The build pins craft-fonts at `2caca391ea8cdd8b3be14a53be8662746ee50385`, the allowed Apache-2.0
Droid Sans Fallback face from AOSP used by upstream PR #879. No font binary is committed here.
The web selection rules retain that face, the Japanese UI face, and Arabic/Telugu/Hebrew faces.
Both Chinese interface languages prefer the Chinese face. Tests cover the web selection rules
and every CJK character in the Simplified/Traditional catalogs. Packages copy the manifest's
license files, including Apache NOTICE, rather than only OFL.txt. Font selection and packaging
code is adapted from @kekeandzeyu's PR #879; newer upstream Hebrew support and system fallback
are preserved. Builds without CRAFT_FONTS_DIR remain supported and cannot promise CJK coverage.
