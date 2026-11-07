//! Filesystem access.
//!
//! Native builds re-export `std::fs` and `walkdir` unchanged, plus [`is_file`] and [`is_dir`]
//! for the `Path` methods of those names. `wasm32` builds route every call through the page's
//! virtual filesystem in `web/vfs.js` behind the same API.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;
