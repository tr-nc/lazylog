// NOTE: this file is kept for backward compatibility with app.rs
// LogItem and LogDetailLevel are now defined in provider::log_item
// Provider-specific formatting should be implemented via LogItemFormatter trait

pub use crate::provider::{LogDetailLevel, LogItem};
