// lazylog - A framework for building terminal-based log viewers
//
// This library provides a provider-based architecture for creating
// terminal log viewing applications with vim-like navigation and
// real-time monitoring capabilities.

pub mod provider;

// Re-export commonly used types
pub use provider::{LogDetailLevel, LogItem, LogProvider, spawn_provider_thread};

// Internal modules (not part of public API)
mod app;
mod app_block;
mod content_line_maker;
mod log_list;
mod log_parser;
mod metadata;
mod status_bar;
mod theme;
mod ui_logger;

// DYEH-specific implementation (internal, but always compiled for now)
mod dyeh;

// Public API for running the application
pub use app::{AppDesc, start, start_with_desc};
