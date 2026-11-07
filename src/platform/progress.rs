//! Terminal progress bars.
//!
//! Native builds re-export `indicatif`. The browser build has no terminal, and `indicatif` would
//! panic there: it spawns a thread for steady ticks and reads `Instant::now`. There the bar only
//! counts, and lines printed above it go to the log.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

#[cfg(target_arch = "wasm32")]
pub(crate) use web::{ProgressBar, ProgressDrawTarget, ProgressStyle};

#[cfg(target_arch = "wasm32")]
mod web {
  use std::{
    borrow::Cow,
    convert::Infallible,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
  };

  /// Where a bar would draw; ignored.
  pub(crate) struct ProgressDrawTarget;

  impl ProgressDrawTarget {
    pub(crate) fn stderr() -> Self {
      Self
    }
  }

  /// How a bar would look; ignored.
  pub(crate) struct ProgressStyle;

  impl ProgressStyle {
    pub(crate) fn with_template(_template: &str) -> Result<Self, Infallible> {
      Ok(Self)
    }

    pub(crate) fn progress_chars(
      self,
      _chars: &str,
    ) -> Self {
      self
    }
  }

  /// A bar that only keeps its position.
  #[derive(Default)]
  pub(crate) struct ProgressBar {
    position: AtomicU64,
  }

  impl ProgressBar {
    pub(crate) fn with_draw_target(
      _length: Option<u64>,
      _target: ProgressDrawTarget,
    ) -> Self {
      Self::default()
    }

    pub(crate) fn set_length(
      &self,
      _length: u64,
    ) {
    }

    pub(crate) fn set_style(
      &self,
      _style: ProgressStyle,
    ) {
    }

    pub(crate) fn enable_steady_tick(
      &self,
      _interval: Duration,
    ) {
    }

    pub(crate) fn set_message(
      &self,
      _message: impl Into<Cow<'static, str>>,
    ) {
    }

    pub(crate) fn inc(
      &self,
      delta: u64,
    ) {
      self.position.fetch_add(delta, Ordering::Relaxed);
    }

    pub(crate) fn position(&self) -> u64 {
      self.position.load(Ordering::Relaxed)
    }

    pub(crate) fn println(
      &self,
      message: impl AsRef<str>,
    ) {
      tracing::info!("{}", message.as_ref());
    }

    pub(crate) fn finish_and_clear(&self) {}
  }
}
