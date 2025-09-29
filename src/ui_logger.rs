use log::{Log, Metadata, Record};
use std::sync::{Arc, Mutex};

pub struct UiLogger {
    logs: Arc<Mutex<Vec<String>>>,
}

impl UiLogger {
    pub fn new(logs: Arc<Mutex<Vec<String>>>) -> Self {
        Self { logs }
    }
}

impl Log for UiLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let log_entry = format!("[{}] {}", record.level(), record.args());
            if let Ok(mut logs) = self.logs.lock() {
                logs.push(log_entry);
                // keep only the last 50 entries to prevent memory bloat
                if logs.len() > 50 {
                    logs.remove(0);
                }
            }
        }
    }
    fn flush(&self) {}
}
