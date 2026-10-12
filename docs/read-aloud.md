# Read out loud

Read out loud plays extracted PDF text through installed Windows speech voices. It uses safe
WinRT SpeechSynthesizer and MediaPlayer calls, with synthesis and audio kept in memory. It does
not upload a document, download a voice, run a shell, or change the PDF. Opening a document or
constructing a player never starts speech.

Open **View → Read out loud**, or **Prepare for accessibility → Read out loud**. Choose the
current page or a page range, an installed voice (or the system default), and a speed from 0.5×
to 2×. Press Read out loud. Pause and Resume preserve the current audio position; Stop cancels
preparation and playback. Closing the player, closing or switching the source tab, or changing
the source PDF stops it. The controls do not create an Undo step or mark the document dirty.

Text preparation runs off the desktop UI thread. A request is limited to 64 pages, 1 MiB of
extracted text and 4,096 Unicode-safe chunks of at most 2,048 bytes each. Pages without text are
listed as skipped. An entirely textless range returns an error directing the user to OCR.
Preparation checks cancellation between pages; extraction already running on one page must
return before that worker exits. Playback commands have a bounded queue; Stop invalidates queued
starts even when the queue is full.

## CLI and MCP

The tools use 1-based pages. `read_aloud_prepare` accepts `{doc, pages?}` and returns the text
chunks and empty page numbers without initializing audio. Omitted pages mean page 1, not the
whole document. Pages must be unique and in document order. Assistive reading is allowed when
copying is prohibited, as specified by ISO 32000-2 §7.6.4.2; this does not change export/copy tools.

- `read_aloud_voices {}` requests voice discovery without speaking.
- `read_aloud_start {doc, pages?, voice?, rate?}` explicitly requests audible playback.
- `read_aloud_control {action: "pause" | "resume" | "stop"}` controls the session's player.
- `read_aloud_status {}` reports playback state, source page, chunk progress, voices and errors.
- `read_aloud_wait {seconds?}` keeps a CLI script alive until playback completes or fails,
  or a 1–60 second timeout (default 30). A persistent MCP session can wait again after a timeout.

Start and control requests are asynchronous. Check status for eventual success or failure.
The player stops when its session exits, so a CLI script must end with `read_aloud_wait` to allow
audio to finish. For example, with `sample.pdf` inside the chosen root:

```json
[
  {"tool":"doc_open","args":{"path":"sample.pdf"}},
  {"tool":"read_aloud_start","args":{"doc":1,"pages":[1],"rate":1.0}},
  {"tool":"read_aloud_wait","args":{"seconds":60}}
]
```

In a fresh CLI session the first opened document has id 1; otherwise use the returned document id.
The same tools are discoverable and callable in compact MCP mode. Speech never starts an MCP
server or adds a network transport.

## Current limits and verification

Playback currently requires Windows with local voices and the Windows media components.
macOS, Linux, FreeBSD and browser playback remain unsupported. Voice and speed choices last
for the current app session; pitch and persisted preferences are not implemented.
Reading order uses PdfCraft's extracted page text, rather than the PDF structure tree: columns,
tables, mathematical text and malformed text maps can read imperfectly. Scans require OCR first.
There is no sentence highlighting or automatic page navigation.

Synthetic tests inject audio backends, so automated tests never speak a document. The explicit
native Windows test below mutes its output and checks that pause freezes playback position,
resume advances it and stop releases the synthesis/stream:

```text
cargo test -p pdfcraft-platform windows_speech_pause_preserves_position_and_stop_releases_stream -- --ignored
```

`cargo run -p pdfcraft-platform --example speech_probe` lists local voices. Adding `--speak`
explicitly runs an audible synthetic smoke check; ordinary builds/tests never run that check.
