//! GPU-sensitive UI tests that share a process-wide shader-compilation lock.

// WARP's ARM64 shader JIT crashes when separate devices compile shaders concurrently. Guarded
// tests acquire this before creating a harness, so it outlives the device.
static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu() -> std::sync::MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

mod document_links;
mod fill_sign;
mod view;
