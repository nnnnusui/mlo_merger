//! Logging work that runs beside other work as one block.

/// Runs `operation` holding back the events it logs on this thread, then logs them together
/// within `span`, so work running beside other work reads as one block.
///
/// Events keep their level but are logged when `operation` returns. Like the log file, only
/// this crate's debug events are kept below info.
pub(crate) fn in_one_block<T>(
  span: tracing::Span,
  operation: impl FnOnce() -> T,
) -> T {
  use tracing::Level;
  use tracing_subscriber::layer::SubscriberExt;

  let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
  let subscriber = tracing_subscriber::registry().with(Capture(events.clone()));
  let result = tracing::subscriber::with_default(subscriber, operation);
  static LOGGING: std::sync::Mutex<()> = std::sync::Mutex::new(());
  let _logging = LOGGING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
  let _span = span.entered();
  let events =
    std::mem::take(&mut *events.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
  for (level, message) in events {
    match level {
      Level::ERROR => tracing::error!("{message}"),
      Level::WARN => tracing::warn!("{message}"),
      Level::INFO => tracing::info!("{message}"),
      _ => tracing::debug!("{message}"),
    }
  }
  result
}

/// Records each event's level and message, with its other fields appended.
struct Capture(std::sync::Arc<std::sync::Mutex<Vec<(tracing::Level, String)>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
  fn enabled(
    &self,
    metadata: &tracing::Metadata<'_>,
    _: tracing_subscriber::layer::Context<'_, S>,
  ) -> bool {
    *metadata.level() <= tracing::Level::INFO
      || (*metadata.level() <= tracing::Level::DEBUG && metadata.target().starts_with("mlo_merger"))
  }

  fn on_event(
    &self,
    event: &tracing::Event<'_>,
    _: tracing_subscriber::layer::Context<'_, S>,
  ) {
    struct Message(String);
    impl tracing::field::Visit for Message {
      fn record_debug(
        &mut self,
        field: &tracing::field::Field,
        value: &dyn std::fmt::Debug,
      ) {
        use std::fmt::Write;
        if field.name() == "message" {
          let _ = write!(self.0, "{value:?}");
        } else {
          let _ = write!(self.0, " {}={value:?}", field.name());
        }
      }
    }
    let mut message = Message(String::new());
    event.record(&mut message);
    self
      .0
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .push((*event.metadata().level(), message.0));
  }
}
