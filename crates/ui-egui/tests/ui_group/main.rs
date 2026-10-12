//! One test binary for the UI tests that are safe to share a process (#454): each links the whole
//! UI and render stack, so grouping them saves a link unit apiece. Tests that hold a GPU device,
//! write screenshots, use process-ID temporary paths or probe installed fonts stay standalone
//! in `tests/`. A file in this folder is only built if it is listed here.

mod animate;
mod bulk_fields;
mod camera;
mod column_select;
mod date_format;
mod dialog_chrome;
mod identity;
mod layer_actions;
mod links;
mod marks;
mod pageboxes;
mod pdf_links;
mod protect;
mod redact;
mod text_select;
mod theme;
mod theme_button;
mod updates;
mod xfa;
