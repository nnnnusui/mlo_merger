//! Filesystem access for the library.
//!
//! Native builds re-export `std::fs` and `walkdir` unchanged. `wasm32` builds run in a browser
//! page and route every call through the page's virtual filesystem in `web/vfs.js`, which keeps
//! the same API so callers compile for both.
//!
//! Library code reaches the filesystem only through this module: `crate::fs::read` rather
//! than `std::fs::read`, and `fs::is_file(&path)` rather than `path.is_file()`, whose `Path`
//! methods call `std::fs` directly. `web/clippy.toml` rejects the direct calls in the `wasm32`
//! build; see `docs/development.md`.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;
