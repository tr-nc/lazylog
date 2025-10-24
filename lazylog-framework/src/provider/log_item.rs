use uuid::Uuid;

/// represents a single log entry from any log source
#[derive(Debug, Clone)]
pub struct LogItem {
    pub id: Uuid,
    pub time: String,
    pub level: String,
    pub origin: String,
    pub tag: String,
    pub content: String,
    pub raw_content: String,
}

impl LogItem {
    pub fn new(
        time: String,
        level: String,
        origin: String,
        tag: String,
        content: String,
        raw_content: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            time,
            level,
            origin,
            tag,
            content,
            raw_content,
        }
    }
}

/// detail level for log display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogDetailLevel {
    ContentOnly, // content only
    Basic,       // [time] content
    Medium,      // [time] [tag] content
    Detailed,    // [time] [tag] [origin] content
    Full,        // [time] [tag] [origin] [level] content
}

impl LogDetailLevel {
    pub fn cycle_forward(&self) -> Self {
        match self {
            LogDetailLevel::ContentOnly => LogDetailLevel::Basic,
            LogDetailLevel::Basic => LogDetailLevel::Medium,
            LogDetailLevel::Medium => LogDetailLevel::Detailed,
            LogDetailLevel::Detailed => LogDetailLevel::Full,
            LogDetailLevel::Full => LogDetailLevel::ContentOnly,
        }
    }

    pub fn cycle_backward(&self) -> Self {
        match self {
            LogDetailLevel::ContentOnly => LogDetailLevel::Full,
            LogDetailLevel::Basic => LogDetailLevel::ContentOnly,
            LogDetailLevel::Medium => LogDetailLevel::Basic,
            LogDetailLevel::Detailed => LogDetailLevel::Medium,
            LogDetailLevel::Full => LogDetailLevel::Detailed,
        }
    }
}

impl From<u8> for LogDetailLevel {
    fn from(value: u8) -> Self {
        match value {
            0 => LogDetailLevel::ContentOnly,
            1 => LogDetailLevel::Basic,
            2 => LogDetailLevel::Medium,
            3 => LogDetailLevel::Detailed,
            _ => LogDetailLevel::Full,
        }
    }
}

impl From<LogDetailLevel> for u8 {
    fn from(value: LogDetailLevel) -> Self {
        match value {
            LogDetailLevel::ContentOnly => 0,
            LogDetailLevel::Basic => 1,
            LogDetailLevel::Medium => 2,
            LogDetailLevel::Detailed => 3,
            LogDetailLevel::Full => 4,
        }
    }
}
