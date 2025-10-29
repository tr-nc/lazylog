// lazylog-framework - A framework for building terminal-based log viewers
//
// This library provides a provider-based architecture for creating
// terminal log viewing applications with vim-like navigation and
// real-time monitoring capabilities.

pub mod provider;

// Re-export commonly used types
pub use provider::{
    LogDetailLevel, LogItem, LogParser, LogProvider, decrement_detail_level,
    increment_detail_level, spawn_provider_thread,
};

// Internal modules (not part of public API but needed for app)
pub(crate) mod app;
pub(crate) mod app_block;
pub(crate) mod content_line_maker;
pub(crate) mod filter;
pub(crate) mod log_list;
pub(crate) mod log_parser;
pub(crate) mod status_bar;
pub(crate) mod theme;
pub(crate) mod ui_logger;

// Public API for running the application
pub use app::{AppDesc, start_with_desc, start_with_provider};
