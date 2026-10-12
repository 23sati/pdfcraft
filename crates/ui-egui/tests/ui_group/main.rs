//! One test binary for UI tests that are safe to share a process (#454): each links the whole UI
//! and render stack, so grouping them saves a link unit apiece. Tests that hold GPU devices, write
//! screenshots, use colliding temporary paths, or probe installed fonts stay standalone in
//! `tests/`. Feature-specific temporary paths may share this process when they cannot collide.

mod accessibility;
mod actions;
mod animate;
mod bulk_fields;
mod buttons;
mod camera;
mod column_select;
mod commands;
mod date_format;
mod dialog_chrome;
mod folders;
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
