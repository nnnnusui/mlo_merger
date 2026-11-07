//! What differs between the native executable and the browser build.
//!
//! Native builds use the operating system through `std`, `walkdir` and `indicatif`. `wasm32`
//! builds run in a browser page with no filesystem, threads or clock of their own; there these
//! modules keep the same API but reach the page's virtual filesystem in `web/vfs.js`, or do
//! nothing, so the rest of the library compiles and behaves the same for both.
//!
//! Library code goes through these modules instead of the `std` items they replace:
//! `platform::fs::read` rather than `std::fs::read`, `fs::is_file(&path)` rather than
//! `path.is_file()` (whose `Path` methods call `std::fs` directly), and `process::current_dir`
//! rather than `std::env::current_dir`. `web/clippy.toml` rejects the direct calls in the
//! `wasm32` build; see `docs/development.md`.

pub mod fs;
pub mod process;
pub(crate) mod progress;
