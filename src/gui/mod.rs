//! The vanilla generator window shown when the executable starts without arguments.
//!
//! It asks for the GTA V directory and `magic.dat`, then generates `vanilla/` next to the
//! executable so a user can upload it to the server without using the command line.

use std::{error::Error, path::PathBuf};

use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

use crate::log;

mod app;
mod game_dir;
mod i18n;
mod log_view;
mod magic;
mod task;

/// Opens the window and blocks until it closes.
pub fn run() -> Result<(), Box<dyn Error>> {
  let base_dir = base_dir();
  detach_own_console();

  let log_view = log_view::LogView::default();
  let log_path = log::log_file_path(&base_dir.join("log"));
  let (file, guard) = log::file_writer(log_path.clone());
  let file = log::tree_layer(file, false).with_filter(EnvFilter::new("info,mlo_merger=debug"));
  let view =
    log::tree_layer(log_view.clone(), false).with_filter(EnvFilter::new("error,mlo_merger=info"));
  tracing_subscriber::registry().with(view).with(file).try_init()?;

  let options = eframe::NativeOptions {
    viewport: eframe::egui::ViewportBuilder::default()
      .with_title("MLO Merger - vanilla generator")
      .with_inner_size([720.0, 540.0])
      .with_min_inner_size([480.0, 360.0]),
    ..Default::default()
  };
  let result = eframe::run_native(
    "mlo_merger",
    options,
    Box::new(move |creation| {
      log_view.set_context(creation.egui_ctx.clone());
      Ok(Box::new(app::VanillaApp::new(base_dir, log_path, log_view)))
    }),
  );
  drop(guard);
  result.map_err(|error| {
    format!("Failed to open the GUI: {error}\nPass --pipeline or another operation to run on the command line.").into()
  })
}

/// The executable's directory, where `vanilla/`, `magic.dat` and logs live.
///
/// Falls back to the working directory when the executable path is unavailable.
fn base_dir() -> PathBuf {
  std::env::current_exe()
    .ok()
    .and_then(|exe| exe.canonicalize().ok())
    .and_then(|exe| exe.parent().map(PathBuf::from))
    .or_else(|| std::env::current_dir().ok())
    .unwrap_or_else(|| PathBuf::from("."))
}

/// Closes the console window Windows opens for a double-clicked console program.
///
/// A console shared with another process, such as the terminal that started this one, is
/// left alone.
#[cfg(windows)]
fn detach_own_console() {
  use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
  let mut processes = [0u32; 2];
  // SAFETY: the buffer holds `processes.len()` process ids.
  unsafe {
    if GetConsoleProcessList(processes.as_mut_ptr(), processes.len() as u32) == 1 {
      FreeConsole();
    }
  }
}

#[cfg(not(windows))]
fn detach_own_console() {}
