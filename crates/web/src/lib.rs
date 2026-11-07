//! Runs `--pipeline`, `--read` and `--restore-source-cache` in a browser page.
//!
//! The library's filesystem calls go to the page's virtual filesystem in `web/vfs.js`. The
//! page fills that filesystem with resources, calls [`run`] with a command line, then reads
//! the files the run wrote to draw them.
//!
//! A panic aborts the WebAssembly instance; the page must load the module again; see
//! `docs/web.md`.
#![cfg(target_arch = "wasm32")]

use std::{cell::RefCell, io};

use mlo_merger::cli::{
  self,
  command::{Command, parse_args_from},
};
use tracing_subscriber::{EnvFilter, fmt::MakeWriter, layer::SubscriberExt};
use wasm_bindgen::prelude::*;

thread_local! {
  /// The `on_log` callback of the running [`run`].
  static ON_LOG: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
}

fn log(
  level: &str,
  message: &str,
) {
  ON_LOG.with(|callback| {
    if let Some(callback) = &*callback.borrow() {
      let _ = callback.call2(&JsValue::NULL, &level.into(), &message.into());
    }
  });
}

/// Runs one command line and returns exactly what the executable writes on standard output,
/// newline included.
///
/// `args` is the argument vector without the executable name, one argument per element, as in
/// `["--pipeline", "-i", "/resources"]`; nothing is split or unquoted. Only `--pipeline`,
/// `--read` and `--restore-source-cache` are available. `on_log(level, message)` receives each
/// log line at `info` and above as it is written. Relative paths resolve against `/`, or the
/// directory given to [`set_current_dir`]. A failure throws an `Error` with the message the
/// executable prints on standard error before exiting with status 1.
///
/// `run` must not be called again from `on_log`; that panics.
#[wasm_bindgen]
pub fn run(
  args: Vec<String>,
  on_log: js_sys::Function,
) -> Result<String, JsError> {
  ON_LOG.with(|callback| *callback.borrow_mut() = Some(on_log));
  std::panic::set_hook(Box::new(|info| log("PANIC", &info.to_string())));
  let subscriber = tracing_subscriber::registry().with(EnvFilter::new("info")).with(
    tracing_subscriber::fmt::layer()
      .with_writer(LogWriters)
      .with_ansi(false)
      .with_target(false)
      .without_time()
      .with_level(false),
  );
  let result = tracing::subscriber::with_default(subscriber, || execute(&args));
  ON_LOG.with(|callback| callback.borrow_mut().take());
  result.map_err(|error| JsError::new(&error))
}

fn execute(args: &[String]) -> Result<String, String> {
  let args = args.iter().map(String::as_str).collect::<Vec<_>>();
  let command = match parse_args_from(&args)? {
    command @ (Command::Pipeline(_) | Command::Read(_) | Command::RestoreSourceCache(_)) => command,
    _ => {
      return Err(
        "Only --pipeline, --read and --restore-source-cache run in the browser".to_string(),
      );
    }
  };
  let output = cli::command::stdout(command);
  output.result.map(|()| output.stdout).map_err(|error| {
    tracing::error!("{error}");
    error.to_string()
  })
}

/// Changes the directory relative paths resolve against.
#[wasm_bindgen]
pub fn set_current_dir(path: &str) -> Result<(), JsError> {
  mlo_merger::platform::process::set_current_dir(path)
    .map_err(|error| JsError::new(&error.to_string()))
}

/// Opens a [`LogWriter`] for each log line.
struct LogWriters;

impl<'a> MakeWriter<'a> for LogWriters {
  type Writer = LogWriter;

  fn make_writer(&'a self) -> LogWriter {
    LogWriter::new("INFO")
  }

  fn make_writer_for(
    &'a self,
    meta: &tracing::Metadata<'_>,
  ) -> LogWriter {
    LogWriter::new(meta.level().as_str())
  }
}

/// Collects one formatted line and hands it to `on_log` when dropped.
struct LogWriter {
  level: &'static str,
  line: Vec<u8>,
}

impl LogWriter {
  fn new(level: &'static str) -> Self {
    Self {
      level,
      line: Vec::new(),
    }
  }
}

impl io::Write for LogWriter {
  fn write(
    &mut self,
    data: &[u8],
  ) -> io::Result<usize> {
    self.line.extend_from_slice(data);
    Ok(data.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

impl Drop for LogWriter {
  fn drop(&mut self) {
    log(self.level, String::from_utf8_lossy(&self.line).trim_end());
  }
}
