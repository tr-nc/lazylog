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
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

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
/// the configured interval (default: 100ms).
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
/// - `poll_interval`: How often to call `poll_logs()` (e.g., 100ms)
///
/// # Returns
///
/// - `JoinHandle`: Thread handle to join on shutdown
/// - `Arc<AtomicBool>`: Stop signal to gracefully terminate the thread
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
/// let (handle, stop_signal) = spawn_provider_thread(
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

            thread::sleep(poll_interval);
        }

        if let Err(e) = provider.stop() {
            log::error!("Failed to stop log provider: {}", e);
        }

        log::debug!("Provider thread stopped");
    });

    (handle, should_stop)
}
