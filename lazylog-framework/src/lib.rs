//! # lazylog-framework
//!
//! A powerful, extensible framework for building terminal-based log viewers with vim-like
//! navigation and real-time monitoring capabilities.
//!
//! ## Overview
//!
//! lazylog-framework provides a provider-based architecture that separates log acquisition
//! from display. You implement the [`LogProvider`] and [`LogParser`] traits to define your
//! log source and formatting, and the framework handles all the TUI rendering, navigation,
//! filtering, and user interaction.
//!
//! ## Core Concepts
//!
//! ### Provider Pattern
//!
//! The framework uses a two-trait system:
//!
//! - **[`LogProvider`]**: Acquires raw log data from any source (files, sockets, APIs, etc.)
//! - **[`LogParser`]**: Parses raw strings into [`LogItem`]s and formats them for display
//!
//! This separation allows you to:
//! - Reuse providers with different parsers
//! - Reuse parsers with different providers
//! - Easily test parsing logic independently
//!
//! ### Ring Buffer
//!
//! Logs flow through a lock-free ring buffer with configurable capacity (default: 16K items).
//! When full, old logs are automatically discarded to prevent unbounded memory growth.
//!
//! ### Non-blocking Architecture
//!
//! The provider runs in a background thread, polling at configurable intervals.
//! The main thread handles UI rendering and user input, keeping the interface responsive.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use lazylog_framework::{LogProvider, LogParser, LogItem, start_with_provider};
//! use anyhow::Result;
//! use std::sync::Arc;
//!
//! // 1. implement LogProvider for your log source
//! struct MyLogProvider {
//!     // your state here (file handle, socket, etc.)
//! }
//!
//! impl LogProvider for MyLogProvider {
//!     fn start(&mut self) -> Result<()> {
//!         // setup resources (open files, connect to streams, etc.)
//!         Ok(())
//!     }
//!
//!     fn stop(&mut self) -> Result<()> {
//!         // cleanup resources
//!         Ok(())
//!     }
//!
//!     fn poll_logs(&mut self) -> Result<Vec<String>> {
//!         // return raw log strings since last poll (non-blocking)
//!         Ok(vec!["2025-01-15 10:30:00 INFO Application started".to_string()])
//!     }
//! }
//!
//! // 2. implement LogParser for your log format
//! struct MyLogParser;
//!
//! impl LogParser for MyLogParser {
//!     fn parse(&self, raw_log: &str) -> Option<LogItem> {
//!         // parse raw string into LogItem
//!         // return None to filter out unwanted logs
//!         Some(LogItem::new(
//!             raw_log.to_string(),  // parsed content
//!             raw_log.to_string(),  // original raw string
//!         ))
//!     }
//!
//!     fn format_preview(&self, item: &LogItem, detail_level: u8) -> String {
//!         // format log for display at given detail level (0-4)
//!         match detail_level {
//!             0 => item.content.clone(),
//!             _ => format!("[{}] {}", item.time, item.content),
//!         }
//!     }
//!
//!     fn get_searchable_text(&self, item: &LogItem, _detail_level: u8) -> String {
//!         // return text that should be searchable at this detail level
//!         item.content.clone()
//!     }
//! }
//!
//! // 3. run the application
//! fn main() -> Result<()> {
//!     use ratatui::{Terminal, backend::CrosstermBackend};
//!     use std::io;
//!
//!     let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
//!     let provider = MyLogProvider { /* ... */ };
//!     let parser = Arc::new(MyLogParser);
//!
//!     lazylog_framework::start_with_provider(&mut terminal, provider, parser)?;
//!     Ok(())
//! }
//! ```
//!
//! ## Advanced Configuration
//!
//! Use [`AppDesc`] to customize behavior:
//!
//! ```rust,no_run
//! use lazylog_framework::{AppDesc, start_with_desc};
//! use std::time::Duration;
//! use std::sync::Arc;
//! # use lazylog_framework::{LogProvider, LogParser, LogItem};
//! # use anyhow::Result;
//! # struct MyProvider;
//! # impl LogProvider for MyProvider {
//! #     fn start(&mut self) -> Result<()> { Ok(()) }
//! #     fn stop(&mut self) -> Result<()> { Ok(()) }
//! #     fn poll_logs(&mut self) -> Result<Vec<String>> { Ok(vec![]) }
//! # }
//! # struct MyParser;
//! # impl LogParser for MyParser {
//! #     fn parse(&self, _: &str) -> Option<LogItem> { None }
//! #     fn format_preview(&self, _: &LogItem, _: u8) -> String { String::new() }
//! #     fn get_searchable_text(&self, _: &LogItem, _: u8) -> String { String::new() }
//! # }
//! # fn main() -> Result<()> {
//! # use ratatui::{Terminal, backend::CrosstermBackend};
//! # use std::io;
//! # let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
//!
//! let parser = Arc::new(MyParser);
//! let mut desc = AppDesc::new(parser.clone());
//! desc.poll_interval = Duration::from_millis(50);  // poll every 50ms
//! desc.ring_buffer_size = 32768;  // 32K log capacity
//! desc.show_debug_logs = true;    // show debug panel
//!
//! let provider = MyProvider;
//! start_with_desc(&mut terminal, provider, desc)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Built-in Features
//!
//! The framework provides a full-featured TUI out of the box:
//!
//! ### Vim-like Navigation
//! - `j`/`k`, `↓`/`↑`: move up/down
//! - `gg`: jump to top, `G`: jump to bottom
//! - `Ctrl+d`/`Ctrl+u`: page down/up
//!
//! ### Filtering
//! - `/`: enter filter mode
//! - Type regex pattern to filter logs in real-time
//! - `Esc`: clear filter
//!
//! ### Detail Levels
//! - `+`/`-`: increase/decrease detail level (0-4)
//! - Allows progressive disclosure of information
//! - Parser controls what each level shows
//!
//! ### Other Features
//! - `y`: yank (copy) selected log to clipboard
//! - `w`: toggle line wrapping
//! - `?`: show help popup
//! - Mouse support: click and scroll
//!
//! ## Performance
//!
//! - **Memory-efficient**: Ring buffer prevents unbounded growth
//! - **Lock-free**: Uses ringbuf crate for zero-allocation producer/consumer
//! - **Lazy rendering**: Only visible logs are formatted and drawn
//! - **Parallel filtering**: Uses rayon for fast regex filtering on large log sets
//!
//! ## Use Cases
//!
//! - **File tailing**: Monitor local log files in real-time
//! - **Container logs**: Stream from Docker/Kubernetes
//! - **Network logs**: Receive syslog over UDP/TCP
//! - **Database logs**: Query and stream from databases
//! - **API logs**: Fetch from logging services (e.g., Elasticsearch)
//! - **Device logs**: Monitor mobile devices (iOS, Android)
//! - **Multi-source aggregation**: Combine multiple log sources
//!
//! ## Examples
//!
//! See the `examples/` directory for complete implementations:
//! - `simple.rs`: Minimal provider that generates dummy logs
//! - `file.rs`: File-based provider with real-time tailing
//! - `structured.rs`: JSON log parsing with detail levels

pub mod provider;

// re-export commonly used types
pub use provider::{
    LogDetailLevel, LogItem, LogParser, LogProvider, decrement_detail_level,
    increment_detail_level, spawn_provider_thread,
};

// internal modules (not part of public API but needed for app)
pub(crate) mod app;
pub(crate) mod app_block;
pub(crate) mod content_line_maker;
pub(crate) mod filter;
pub(crate) mod log_list;
pub(crate) mod log_parser;
pub(crate) mod theme;
pub(crate) mod ui_logger;
pub mod status_bar;

// public API for running the application
pub use app::{AppDesc, start_with_desc, start_with_provider};
