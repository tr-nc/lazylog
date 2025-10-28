mod log_item;

pub use log_item::{
    LogDetailLevel, LogItem, LogItemFormatter, decrement_detail_level, increment_detail_level,
};

use anyhow::Result;
use ringbuf::traits::Producer;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

/// trait for providing log items from various sources
pub trait LogProvider: Send {
    /// start the provider (setup resources, spawn threads, etc.)
    fn start(&mut self) -> Result<()>;

    /// stop the provider (cleanup, join threads, etc.)
    fn stop(&mut self) -> Result<()>;

    /// poll for new logs (non-blocking)
    fn poll_logs(&mut self) -> Result<Vec<LogItem>>;
}

/// spawns a provider thread that continuously polls logs and pushes to ring buffer
pub fn spawn_provider_thread<P>(
    mut provider: P,
    mut producer: impl Producer<Item = LogItem> + Send + 'static,
    poll_interval: Duration,
) -> (thread::JoinHandle<()>, Arc<AtomicBool>)
where
    P: LogProvider + 'static,
{
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = should_stop.clone();

    let handle = thread::spawn(move || {
        if let Err(e) = provider.start() {
            log::error!("Failed to start log provider: {}", e);
            return;
        }

        log::debug!("Provider thread started");

        while !should_stop_clone.load(Ordering::Relaxed) {
            match provider.poll_logs() {
                Ok(logs) => {
                    for log in logs {
                        if producer.try_push(log).is_err() {
                            log::debug!("Ring buffer full, dropping log");
                        }
                    }
                }
                Err(e) => {
                    log::debug!("Provider poll error: {}", e);
                }
            }

            thread::sleep(poll_interval);
        }

        if let Err(e) = provider.stop() {
            log::error!("Failed to stop log provider: {}", e);
        }

        log::debug!("Provider thread stopped");
    });

    (handle, should_stop)
}
