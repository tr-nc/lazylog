//! Provider and parser traits for log acquisition and formatting.
//!
//! This module defines the core abstractions for log sources and formatting:
//!
//! - [`LogProvider`]: Acquires raw log data from any source
//! - [`LogParser`]: Parses and formats logs for display
//! - [`LogItem`]: Structured representation of a log entry
//!
//! # Architecture
//!
//! The provider pattern separates concerns:
//!
//! ```text
//! ┌──────────────┐    poll_logs()      ┌─────────────┐
//! │ LogProvider  │ ──────────────────> │ Vec<String> │ (raw logs)
//! └──────────────┘                     └──────┬──────┘
//!                                              │
//!                                              │ parse()
//!                                              │
//! ┌──────────────┐    format_preview()  ┌─────▼──────┐
//! │  LogParser   │ <─────────────────── │  LogItem   │
//! └──────────────┘                      └────────────┘
//! ```
//!
//! This design allows:
//! - Same provider with different parsers (e.g., JSON vs plain text)
//! - Same parser with different providers (e.g., file vs network)
//! - Easy testing of parsing logic independently

mod log_item;

pub use log_item::{
    LogDetailLevel, LogItem, LogParser, decrement_detail_level, increment_detail_level,
};

use anyhow::Result;
use ringbuf::traits::Producer;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

/// High-level connection state shared by all providers that can report one.
///
/// Platform adapters own the evidence needed to choose a disconnect reason;
/// callers only need this stable, platform-neutral state model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderStatus {
    Connecting,
    Connected,
    Disconnected(ProviderDisconnectReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderDisconnectReason {
    UsbDisconnected,
    DeviceDisconnected,
    TargetExited,
    CaptureFailed,
    Unknown,
}

impl ProviderStatus {
    /// Stable user-facing label shared by interactive and non-interactive modes.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Connecting => "连接中",
            Self::Connected => "已连接",
            Self::Disconnected(reason) => match reason {
                ProviderDisconnectReason::UsbDisconnected => "USB 已拔出",
                ProviderDisconnectReason::DeviceDisconnected => "设备已断开",
                ProviderDisconnectReason::TargetExited => "App 已退出",
                ProviderDisconnectReason::CaptureFailed => "采集连接异常",
                ProviderDisconnectReason::Unknown => "连接已断开",
            },
        }
    }

    pub const fn is_disconnected(self) -> bool {
        matches!(self, Self::Disconnected(_))
    }
}

/// Trait for acquiring raw log data from any source.
///
/// Implement this trait to define where logs come from (files, network, APIs, etc.).
/// The provider is responsible for:
/// - Opening and managing resources (files, sockets, connections)
/// - Polling for new log data at regular intervals
/// - Returning raw log strings (decoded, but not parsed)
/// - Cleaning up resources on shutdown
///
/// # Non-blocking Contract
///
/// `poll_logs()` **must be non-blocking**. If no new logs are available, return
/// an empty `Vec` immediately. The framework calls `poll_logs()` repeatedly at
/// the configured interval.
///
/// # Thread Safety
///
/// Providers run in a dedicated background thread via [`spawn_provider_thread`].
/// The trait requires `Send` to allow safe transfer across thread boundaries.
///
/// # Examples
///
/// ## Simple File Provider
///
/// ```rust
/// use lazylog_framework::LogProvider;
/// use anyhow::Result;
/// use std::fs::File;
/// use std::io::{BufRead, BufReader};
///
/// struct FileProvider {
///     reader: Option<BufReader<File>>,
///     path: String,
/// }
///
/// impl FileProvider {
///     fn new(path: impl Into<String>) -> Self {
///         Self {
///             reader: None,
///             path: path.into(),
///         }
///     }
/// }
///
/// impl LogProvider for FileProvider {
///     fn start(&mut self) -> Result<()> {
///         let file = File::open(&self.path)?;
///         self.reader = Some(BufReader::new(file));
///         Ok(())
///     }
///
///     fn stop(&mut self) -> Result<()> {
///         self.reader = None;
///         Ok(())
///     }
///
///     fn poll_logs(&mut self) -> Result<Vec<String>> {
///         let mut logs = Vec::new();
///         if let Some(reader) = &mut self.reader {
///             let mut line = String::new();
///             while reader.read_line(&mut line)? > 0 {
///                 if !line.trim().is_empty() {
///                     logs.push(line.trim().to_string());
///                 }
///                 line.clear();
///             }
///         }
///         Ok(logs)
///     }
/// }
/// ```
pub trait LogProvider: Send {
    /// Initialize the provider and acquire resources.
    ///
    /// Called once at startup before any `poll_logs()` calls.
    /// Use this to:
    /// - Open files, sockets, or connections
    /// - Authenticate with APIs
    /// - Spawn internal threads (if needed)
    /// - Perform any one-time setup
    ///
    /// # Errors
    ///
    /// Return an error if initialization fails. The framework will abort startup.
    fn start(&mut self) -> Result<()>;

    /// Clean up resources and shut down the provider.
    ///
    /// Called once when the application exits or the provider is stopped.
    /// Use this to:
    /// - Close files, sockets, or connections
    /// - Join spawned threads
    /// - Flush buffers
    /// - Release any held resources
    ///
    /// # Errors
    ///
    /// Errors are logged but do not prevent shutdown.
    fn stop(&mut self) -> Result<()>;

    /// Poll for new log data (non-blocking).
    ///
    /// Called repeatedly at the configured interval (see [`crate::AppDesc::poll_interval`]).
    /// Return any new logs since the last call as raw strings.
    ///
    /// # Contract
    ///
    /// - **Must be non-blocking**: If no logs are available, return `Ok(vec![])` immediately
    /// - **Return raw strings**: Logs should be decoded (e.g., UTF-8) but not parsed
    /// - **Handle partial data**: Buffer incomplete lines between calls if needed
    /// - **Manage backpressure**: If the source produces logs faster than you can consume,
    ///   consider buffering or dropping logs
    ///
    /// # Errors
    ///
    /// Transient errors (e.g., network timeouts) are logged but do not stop polling.
    /// Fatal errors (e.g., file deleted) should be returned to stop the provider.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use lazylog_framework::LogProvider;
    /// # use anyhow::Result;
    /// # struct MyProvider { buffer: Vec<String> }
    /// impl LogProvider for MyProvider {
    ///     # fn start(&mut self) -> Result<()> { Ok(()) }
    ///     # fn stop(&mut self) -> Result<()> { Ok(()) }
    ///     fn poll_logs(&mut self) -> Result<Vec<String>> {
    ///         // drain internal buffer and return
    ///         Ok(self.buffer.drain(..).collect())
    ///     }
    /// }
    /// ```
    fn poll_logs(&mut self) -> Result<Vec<String>>;

    /// Return the provider's current connection state, if it has one.
    ///
    /// Providers without a meaningful live connection can use the default.
    /// The call must be non-blocking; platform probing belongs inside the
    /// provider implementation, not in the UI thread.
    fn status(&self) -> Option<ProviderStatus> {
        None
    }
}

/// Spawns a background thread that runs a provider and feeds logs into a ring buffer.
///
/// This function is the glue between providers and the framework. It:
/// 1. Starts the provider
/// 2. Polls it at regular intervals
/// 3. Parses raw strings into [`LogItem`]s
/// 4. Pushes items into the ring buffer for display
///
/// # Parameters
///
/// - `provider`: Your [`LogProvider`] implementation
/// - `parser`: A [`LogParser`] to parse raw strings
/// - `producer`: Ring buffer producer (framework-managed)
/// - `poll_interval`: How often to call `poll_logs()`
///
/// # Returns
///
/// - `JoinHandle`: Thread handle to join on shutdown
/// - `Arc<AtomicBool>`: Stop signal to gracefully terminate the thread
/// - Shared current provider status for UI rendering
///
/// # Lifecycle
///
/// 1. Calls `provider.start()`
/// 2. Loops: `poll_logs()` → `parser.parse()` → push to ring buffer
/// 3. Sleeps for `poll_interval` between polls
/// 4. On stop signal: calls `provider.stop()` and exits
///
/// # Errors
///
/// - Errors from `start()` are logged and abort the thread
/// - Errors from `poll_logs()` are logged but polling continues
/// - Errors from `stop()` are logged but don't prevent shutdown
///
/// # Examples
///
/// This is typically called by the framework internally, but you can use it manually:
///
/// ```rust,no_run
/// use lazylog_framework::{LogProvider, LogParser, LogItem, spawn_provider_thread};
/// use std::sync::Arc;
/// use std::time::Duration;
/// use ringbuf::{HeapRb, traits::Split};
/// # use anyhow::Result;
/// # struct MyProvider;
/// # impl LogProvider for MyProvider {
/// #     fn start(&mut self) -> Result<()> { Ok(()) }
/// #     fn stop(&mut self) -> Result<()> { Ok(()) }
/// #     fn poll_logs(&mut self) -> Result<Vec<String>> { Ok(vec![]) }
/// # }
/// # struct MyParser;
/// # impl LogParser for MyParser {
/// #     fn parse(&self, _: &str) -> Option<LogItem> { None }
/// #     fn format_preview(&self, _: &LogItem, _: u8) -> String { String::new() }
/// #     fn get_searchable_text(&self, _: &LogItem, _: u8) -> String { String::new() }
/// # }
///
/// let provider = MyProvider;
/// let parser = Arc::new(MyParser);
/// let ring_buffer = HeapRb::<LogItem>::new(1024);
/// let (producer, consumer) = ring_buffer.split();
///
/// let (handle, stop_signal, _provider_status) = spawn_provider_thread(
///     provider,
///     parser,
///     producer,
///     Duration::from_millis(100),
/// );
///
/// // later...
/// stop_signal.store(true, std::sync::atomic::Ordering::Relaxed);
/// handle.join().ok();
/// ```
pub fn spawn_provider_thread<P>(
    mut provider: P,
    parser: Arc<dyn LogParser>,
    mut producer: impl Producer<Item = LogItem> + Send + 'static,
    poll_interval: Duration,
) -> (
    thread::JoinHandle<()>,
    Arc<AtomicBool>,
    Arc<Mutex<Option<ProviderStatus>>>,
)
where
    P: LogProvider + 'static,
{
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = should_stop.clone();
    let provider_status = Arc::new(Mutex::new(provider.status()));
    let provider_status_clone = provider_status.clone();

    let handle = thread::spawn(move || {
        if let Err(e) = provider.start() {
            log::error!("Failed to start log provider: {}", e);
            publish_provider_status(
                &provider_status_clone,
                Some(ProviderStatus::Disconnected(
                    ProviderDisconnectReason::CaptureFailed,
                )),
            );
            return;
        }

        log::debug!("Provider thread started");
        publish_provider_status(&provider_status_clone, provider.status());

        while !should_stop_clone.load(Ordering::Relaxed) {
            match provider.poll_logs() {
                Ok(raw_logs) => {
                    for raw_log in raw_logs {
                        // parser may return None if it acts as a filter
                        if let Some(log_item) = parser.parse(&raw_log)
                            && producer.try_push(log_item).is_err()
                        {
                            log::debug!("Ring buffer full, dropping log");
                        }
                    }
                }
                Err(e) => {
                    log::debug!("Provider poll error: {}", e);
                }
            }

            publish_provider_status(&provider_status_clone, provider.status());

            sleep_interruptible(poll_interval, &should_stop_clone);
        }

        if let Err(e) = provider.stop() {
            log::error!("Failed to stop log provider: {}", e);
        }

        log::debug!("Provider thread stopped");
    });

    (handle, should_stop, provider_status)
}

fn publish_provider_status(
    shared: &Arc<Mutex<Option<ProviderStatus>>>,
    status: Option<ProviderStatus>,
) {
    if let Ok(mut current) = shared.lock()
        && *current != status
    {
        *current = status;
    }
}

fn sleep_interruptible(duration: Duration, should_stop: &AtomicBool) {
    const CHECK_INTERVAL_MS: u64 = 25;
    let check_interval = Duration::from_millis(CHECK_INTERVAL_MS);

    if duration <= check_interval {
        thread::sleep(duration);
        return;
    }

    let mut elapsed = Duration::ZERO;
    while elapsed < duration {
        if should_stop.load(Ordering::Relaxed) {
            break;
        }
        let sleep_time = check_interval.min(duration - elapsed);
        thread::sleep(sleep_time);
        elapsed += sleep_time;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::{HeapRb, traits::Split};
    use std::time::Instant;

    struct NullParser;

    impl LogParser for NullParser {
        fn parse(&self, _raw_log: &str) -> Option<LogItem> {
            None
        }

        fn format_preview(&self, _item: &LogItem, _detail_level: LogDetailLevel) -> String {
            String::new()
        }

        fn get_searchable_text(&self, _item: &LogItem, _detail_level: LogDetailLevel) -> String {
            String::new()
        }
    }

    struct StatusProvider {
        status: ProviderStatus,
    }

    impl LogProvider for StatusProvider {
        fn start(&mut self) -> Result<()> {
            self.status = ProviderStatus::Connected;
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            Ok(())
        }

        fn poll_logs(&mut self) -> Result<Vec<String>> {
            self.status = ProviderStatus::Disconnected(ProviderDisconnectReason::TargetExited);
            Ok(Vec::new())
        }

        fn status(&self) -> Option<ProviderStatus> {
            Some(self.status)
        }
    }

    #[test]
    fn status_labels_are_shared_across_frontends() {
        assert_eq!(ProviderStatus::Connecting.label(), "连接中");
        assert_eq!(ProviderStatus::Connected.label(), "已连接");
        assert_eq!(
            ProviderStatus::Disconnected(ProviderDisconnectReason::UsbDisconnected).label(),
            "USB 已拔出"
        );
        assert_eq!(
            ProviderStatus::Disconnected(ProviderDisconnectReason::TargetExited).label(),
            "App 已退出"
        );
    }

    #[test]
    fn provider_thread_publishes_terminal_status() {
        let provider = StatusProvider {
            status: ProviderStatus::Connecting,
        };
        let ring_buffer = HeapRb::<LogItem>::new(4);
        let (producer, _consumer) = ring_buffer.split();
        let (handle, stop, status) = spawn_provider_thread(
            provider,
            Arc::new(NullParser),
            producer,
            Duration::from_millis(1),
        );

        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            let current = status.lock().ok().and_then(|status| *status);
            if current
                == Some(ProviderStatus::Disconnected(
                    ProviderDisconnectReason::TargetExited,
                ))
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "provider status was not published"
            );
            thread::sleep(Duration::from_millis(1));
        }

        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }
}
