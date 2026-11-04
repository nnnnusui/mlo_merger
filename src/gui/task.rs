//! Background work the window waits on without blocking its event loop.

use std::{
  sync::mpsc::{self, Receiver, TryRecvError},
  thread,
};

use eframe::egui;

/// Work running on its own thread, ending in success or an error message.
pub struct Task {
  receiver: Receiver<Result<(), String>>,
}

impl Task {
  /// Runs `work` on a new thread and repaints `context` when it ends, including by panic.
  pub fn spawn<E: std::fmt::Display>(
    context: &egui::Context,
    work: impl FnOnce() -> Result<(), E> + Send + 'static,
  ) -> Self {
    let (sender, receiver) = mpsc::channel();
    let context = context.clone();
    thread::spawn(move || {
      let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(panic) => {
          let message = panic
            .downcast_ref::<&str>()
            .map(|text| text.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_string());
          Err(format!("Internal error: {message}"))
        }
      };
      if let Err(error) = &result {
        tracing::error!("{error}");
      }
      let _ = sender.send(result);
      context.request_repaint();
    });
    Self {
      receiver,
    }
  }

  /// The outcome once the work has ended.
  pub fn poll(&self) -> Option<Result<(), String>> {
    match self.receiver.try_recv() {
      Ok(result) => Some(result),
      Err(TryRecvError::Empty) => None,
      Err(TryRecvError::Disconnected) => Some(Err("The task ended unexpectedly".to_string())),
    }
  }
}
