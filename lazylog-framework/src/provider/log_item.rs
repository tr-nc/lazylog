use chrono::Local;
use std::collections::HashMap;
use uuid::Uuid;

/// represents a single log entry from any log source
#[derive(Debug, Clone)]
pub struct LogItem {
    pub id: Uuid,
    pub time: String,
    pub content: String,
    pub raw_content: String,
    pub metadata: HashMap<String, String>,
}

impl LogItem {
    pub fn new(content: String, raw_content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            time: Local::now().format("%H:%M:%S%.3f").to_string(),
            content,
            raw_content,
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }
}

/// detail level for log display (0-255, provider-specific interpretation)
pub type LogDetailLevel = u8;

/// trait for formatting log items in a provider-specific way
pub trait LogItemFormatter: Send + Sync {
    /// format a log item for preview display based on detail level
    /// detail_level: 0 = minimal, higher = more detailed
    fn format_preview(&self, item: &LogItem, detail_level: LogDetailLevel) -> String;

    /// get searchable text from log item based on detail level
    /// used for filtering - should include all fields that should be searchable at this level
    fn get_searchable_text(&self, item: &LogItem, detail_level: LogDetailLevel) -> String;

    /// get text to yank/copy to clipboard
    fn make_yank_content(&self, item: &LogItem) -> String {
        item.raw_content.clone()
    }

    /// get maximum detail level supported by this formatter
    fn max_detail_level(&self) -> LogDetailLevel {
        4 // default: 5 levels (0-4)
    }
}

/// helper functions for detail level navigation
pub fn increment_detail_level(level: LogDetailLevel, max: LogDetailLevel) -> LogDetailLevel {
    level.saturating_add(1).min(max)
}

pub fn decrement_detail_level(level: LogDetailLevel) -> LogDetailLevel {
    level.saturating_sub(1)
}
