use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::subscriber;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;

/// The `error!` events the stack logs while a scenario runs: a run that keeps every invariant
/// logs none.
#[derive(Debug, Clone, Default)]
pub(crate) struct ErrorCapture {
    errors: Arc<Mutex<Vec<String>>>,
}

impl ErrorCapture {
    /// Runs `run` with this capture as the thread's subscriber.
    pub(crate) fn around<T>(&self, run: impl FnOnce() -> T) -> T {
        let registry = tracing_subscriber::registry().with(self.clone());
        subscriber::with_default(registry, run)
    }

    pub(crate) fn errors(&self) -> Vec<String> {
        self.errors.lock().unwrap().clone()
    }
}

impl<S: Subscriber> Layer<S> for ErrorCapture {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        if *metadata.level() != Level::ERROR || !metadata.target().starts_with("campfire_net") {
            return;
        }
        let mut message = MessageField(String::new());
        event.record(&mut message);
        self.errors
            .lock()
            .unwrap()
            .push(format!("{}: {}", metadata.target(), message.0));
    }
}

#[derive(Debug)]
struct MessageField(String);

impl Visit for MessageField {
    fn record_debug(&mut self, field: &Field, value: &dyn core::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}
