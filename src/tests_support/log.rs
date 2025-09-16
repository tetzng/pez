#[cfg(test)]
use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
};

use std::sync::OnceLock;
#[cfg(test)]
use tracing::{Event, Subscriber};
#[cfg(test)]
use tracing_subscriber::{Layer, Registry, layer::Context, prelude::*};

#[cfg(test)]
struct MessageVisitor {
    message: Option<String>,
}

#[cfg(test)]
impl MessageVisitor {
    fn new() -> Self {
        Self { message: None }
    }
}

#[cfg(test)]
impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}").trim_matches('"').to_string());
        }
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }
}

#[cfg(test)]
struct MessageLayer {
    sink: Arc<Mutex<Vec<String>>>,
}

#[cfg(test)]
impl<S> Layer<S> for MessageLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::new();
        event.record(&mut visitor);
        if let Some(msg) = visitor.message
            && let Ok(mut v) = self.sink.lock()
        {
            v.push(msg);
        }
    }
}

/// Capture tracing messages produced within `f` and return them as Vec<String>.
#[cfg(test)]
pub fn capture_logs<F, R>(f: F) -> (Vec<String>, R)
where
    F: FnOnce() -> R,
{
    let sink = Arc::new(Mutex::new(Vec::<String>::new()));
    let layer = MessageLayer { sink: sink.clone() };
    let subscriber = Registry::default().with(layer);
    let result = tracing::subscriber::with_default(subscriber, f);
    let logs = Arc::try_unwrap(sink).unwrap().into_inner().unwrap();
    (logs, result)
}

#[cfg(test)]
pub fn env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}
