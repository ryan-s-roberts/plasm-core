//! Typed capture of `tracing` events for alternate-screen TUIs (level, target, message).

use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use tracing::field::{Field, Visit};
use tracing::Level;
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// One log line captured from a `tracing` event (not fmt-formatted).
#[derive(Clone, Debug)]
pub struct TuiLogRecord {
    pub timestamp: SystemTime,
    pub level: Level,
    pub target: String,
    pub message: String,
}

pub type TuiLogCallback = Arc<dyn Fn(TuiLogRecord) + Send + Sync>;

#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
}

impl MessageVisitor {
    fn finish(self) -> String {
        self.message.unwrap_or_default()
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            let s = format!("{value:?}");
            self.message = Some(s.trim_matches('"').to_string());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }
}

/// Layer that forwards filtered events to `callback` when set.
#[derive(Clone)]
pub struct TuiCaptureLayer {
    callback: Option<TuiLogCallback>,
}

pub fn layer(callback: Option<TuiLogCallback>) -> TuiCaptureLayer {
    TuiCaptureLayer { callback }
}

impl<S> Layer<S> for TuiCaptureLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let Some(cb) = &self.callback else {
            return;
        };
        let meta = event.metadata();
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        (cb)(TuiLogRecord {
            timestamp: SystemTime::now(),
            level: *meta.level(),
            target: meta.target().to_string(),
            message: visitor.finish(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    #[test]
    fn capture_layer_records_level_target_and_message() {
        let records: Arc<Mutex<Vec<TuiLogRecord>>> = Arc::new(Mutex::new(Vec::new()));
        let records_cb = Arc::clone(&records);
        let cb: TuiLogCallback = Arc::new(move |rec| records_cb.lock().unwrap().push(rec));

        let _guard = tracing_subscriber::registry()
            .with(layer(Some(cb)))
            .with(EnvFilter::new("info"))
            .set_default();

        tracing::warn!(target: "test_target", "hello appliance");

        let got = records.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].level, Level::WARN);
        assert_eq!(got[0].target, "test_target");
        assert_eq!(got[0].message, "hello appliance");
    }
}
