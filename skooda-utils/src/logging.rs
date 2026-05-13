use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

pub fn init_logging() {
    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .compact();

    let kmsg_layer = KmsgLayer::new();

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with(fmt_layer)
        .with(kmsg_layer)
        .init();
}

struct KmsgLayer {
    file: Option<Mutex<std::fs::File>>,
}

impl KmsgLayer {
    fn new() -> Self {
        let file = OpenOptions::new()
            .write(true)
            .open("/dev/kmsg")
            .ok()
            .map(Mutex::new);
        
        Self { file }
    }
}

impl<S> tracing_subscriber::Layer<S> for KmsgLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if let Some(mutex) = &self.file {
            let mut file = mutex.lock().unwrap();
            let mut visitor = KmsgVisitor::new();
            event.record(&mut visitor);
            
            let level = match *event.metadata().level() {
                tracing::Level::ERROR => 3,
                tracing::Level::WARN => 4,
                tracing::Level::INFO => 5,
                tracing::Level::DEBUG => 6,
                tracing::Level::TRACE => 7,
            };

            // Remove quotes from debug formatting if it's a string
            let clean_msg = visitor.message.trim_matches('"');
            let msg = format!("<{}>[skooda] {}\n", level, clean_msg);
            let _ = file.write_all(msg.as_bytes());
        }
    }
}

struct KmsgVisitor {
    message: String,
}

impl KmsgVisitor {
    fn new() -> Self {
        Self { message: String::new() }
    }
}

impl tracing::field::Visit for KmsgVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
}
