use chrono::Local;
use std::collections::HashMap;
use uuid::Uuid;

/// A structured representation of a single log entry.
///
/// `LogItem` is the core data structure passed from providers to the UI. It contains:
/// - `id`: Unique identifier for deduplication and selection tracking
/// - `time`: Human-readable timestamp (formatted by parser or auto-generated)
/// - `content`: Parsed/formatted log message
/// - `raw_content`: Original unparsed log line
/// - `metadata`: Extensible key-value storage for custom fields
///
/// # Metadata Pattern
///
/// Use metadata to store structured fields like log level, module, tags, etc.:
///
/// ```rust
/// use lazylog_framework::LogItem;
///
/// let log = LogItem::new(
///     "Application started".to_string(),
///     "2025-01-15 10:30:00 INFO main.rs Application started".to_string(),
/// )
/// .with_metadata("level", "INFO")
/// .with_metadata("module", "main")
/// .with_metadata("severity", "1");
///
/// assert_eq!(log.get_metadata("level"), Some("INFO"));
/// ```
///
/// The parser can then use metadata to control formatting at different detail levels.
#[derive(Debug, Clone)]
pub struct LogItem {
    /// unique identifier (auto-generated)
    pub id: Uuid,

    /// human-readable timestamp (e.g., "14:30:25.123")
    pub time: String,

    /// parsed/formatted log content
    pub content: String,

    /// original raw log line
    pub raw_content: String,

    /// extensible metadata (level, module, tags, etc.)
    pub metadata: HashMap<String, String>,
}

impl LogItem {
    /// Creates a new log item with auto-generated ID and timestamp.
    ///
    /// # Parameters
    ///
    /// - `content`: The parsed log message (can be same as `raw_content` for simple logs)
    /// - `raw_content`: The original unparsed log line
    ///
    /// # Examples
    ///
    /// ```rust
    /// use lazylog_framework::LogItem;
    ///
    /// // simple log (content == raw)
    /// let log = LogItem::new(
    ///     "Hello world".to_string(),
    ///     "Hello world".to_string(),
    /// );
    ///
    /// // parsed log (content extracted from raw)
    /// let raw = "2025-01-15 10:30:00 INFO Application started";
    /// let log = LogItem::new(
    ///     "Application started".to_string(),
    ///     raw.to_string(),
    /// );
    /// ```
    pub fn new(content: String, raw_content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            time: Local::now().format("%H:%M:%S%.3f").to_string(),
            content,
            raw_content,
            metadata: HashMap::new(),
        }
    }

    /// Adds metadata to the log item (builder pattern).
    ///
    /// Common metadata keys:
    /// - `level`: Log level (e.g., "INFO", "ERROR", "DEBUG")
    /// - `module`: Module or component name
    /// - `tag`: Category or tag
    /// - `severity`: Numeric severity (for sorting/filtering)
    /// - `thread`: Thread ID
    /// - `file`: Source file name
    ///
    /// # Examples
    ///
    /// ```rust
    /// use lazylog_framework::LogItem;
    ///
    /// let log = LogItem::new("msg".into(), "raw".into())
    ///     .with_metadata("level", "ERROR")
    ///     .with_metadata("module", "auth")
    ///     .with_metadata("severity", "3");
    /// ```
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Retrieves metadata by key.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use lazylog_framework::LogItem;
    ///
    /// let log = LogItem::new("msg".into(), "raw".into())
    ///     .with_metadata("level", "INFO");
    ///
    /// assert_eq!(log.get_metadata("level"), Some("INFO"));
    /// assert_eq!(log.get_metadata("missing"), None);
    /// ```
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }
}

/// Detail level for log display (0-255, parser-defined).
///
/// Higher levels show more information. Common convention:
/// - **0**: Content only (minimal)
/// - **1**: Time + content
/// - **2**: Time + tag + content
/// - **3**: Time + tag + module + content
/// - **4**: All fields (maximum detail)
///
/// Parsers define what each level shows via [`LogParser::format_preview`].
pub type LogDetailLevel = u8;

/// Trait for parsing raw log strings and formatting them for display.
///
/// `LogParser` bridges the gap between raw log strings (from [`LogProvider`](crate::LogProvider))
/// and structured [`LogItem`]s for the UI. It handles both:
/// 1. **Parsing**: Converting raw strings → structured data
/// 2. **Formatting**: Rendering log items at different detail levels
/// 3. **Filtering**: Extracting searchable text for regex matching
///
/// # Design Philosophy
///
/// Separating parsing from providers allows:
/// - Same parser for multiple providers (e.g., JSON parser for files + network)
/// - Same provider for multiple parsers (e.g., file provider with JSON/plain/syslog parsers)
/// - Easy testing of parsing logic without I/O
///
/// # Thread Safety
///
/// Parsers must be `Send + Sync` because they're shared across threads via `Arc<dyn LogParser>`.
///
/// # Examples
///
/// ## Simple Plain Text Parser
///
/// ```rust
/// use lazylog_framework::{LogParser, LogItem, LogDetailLevel};
///
/// struct PlainParser;
///
/// impl LogParser for PlainParser {
///     fn parse(&self, raw_log: &str) -> Option<LogItem> {
///         Some(LogItem::new(raw_log.to_string(), raw_log.to_string()))
///     }
///
///     fn format_preview(&self, item: &LogItem, level: LogDetailLevel) -> String {
///         match level {
///             0 => item.content.clone(),
///             _ => format!("[{}] {}", item.time, item.content),
///         }
///     }
///
///     fn get_searchable_text(&self, item: &LogItem, _level: LogDetailLevel) -> String {
///         item.content.clone()
///     }
/// }
/// ```
///
/// ## Structured JSON Parser
///
/// ```rust
/// use lazylog_framework::{LogParser, LogItem, LogDetailLevel};
///
/// struct JsonParser;
///
/// impl LogParser for JsonParser {
///     fn parse(&self, raw_log: &str) -> Option<LogItem> {
///         // parse JSON and extract fields
///         // return None to filter out invalid logs
///         let json: serde_json::Value = serde_json::from_str(raw_log).ok()?;
///
///         Some(LogItem::new(
///             json["message"].as_str()?.to_string(),
///             raw_log.to_string(),
///         )
///         .with_metadata("level", json["level"].as_str().unwrap_or("INFO"))
///         .with_metadata("module", json["module"].as_str().unwrap_or("")))
///     }
///
///     fn format_preview(&self, item: &LogItem, level: LogDetailLevel) -> String {
///         match level {
///             0 => item.content.clone(),
///             1 => format!("[{}] {}", item.time, item.content),
///             2 => format!("[{}] [{}] {}",
///                 item.time,
///                 item.get_metadata("level").unwrap_or(""),
///                 item.content),
///             _ => format!("[{}] [{}] [{}] {}",
///                 item.time,
///                 item.get_metadata("level").unwrap_or(""),
///                 item.get_metadata("module").unwrap_or(""),
///                 item.content),
///         }
///     }
///
///     fn get_searchable_text(&self, item: &LogItem, level: LogDetailLevel) -> String {
///         // include more fields at higher detail levels
///         if level >= 2 {
///             format!("{} {} {}",
///                 item.get_metadata("level").unwrap_or(""),
///                 item.get_metadata("module").unwrap_or(""),
///                 item.content)
///         } else {
///             item.content.clone()
///         }
///     }
/// }
/// ```
pub trait LogParser: Send + Sync {
    /// Parses a raw log string into a structured [`LogItem`].
    ///
    /// This is called for every log line from the provider. Implement your parsing logic here:
    /// - Extract timestamp, level, module, etc. into metadata
    /// - Clean up or format the content field
    /// - Return `None` to **filter out** logs you want to ignore
    ///
    /// # Filtering
    ///
    /// Returning `None` acts as a filter. Common use cases:
    /// - Ignore debug logs in production
    /// - Filter by log level
    /// - Skip malformed/unparseable lines
    ///
    /// # Performance
    ///
    /// This is called frequently (potentially thousands of times per second).
    /// Keep parsing logic efficient. Avoid:
    /// - Allocating unnecessarily
    /// - Complex regex (use simple string matching where possible)
    /// - Blocking I/O
    ///
    /// # Examples
    ///
    /// ```rust
    /// use lazylog_framework::{LogParser, LogItem};
    /// # struct MyParser;
    /// impl LogParser for MyParser {
    ///     fn parse(&self, raw_log: &str) -> Option<LogItem> {
    ///         // filter out empty lines
    ///         if raw_log.trim().is_empty() {
    ///             return None;
    ///         }
    ///
    ///         // filter by level
    ///         if raw_log.contains("DEBUG") {
    ///             return None; // skip debug logs
    ///         }
    ///
    ///         Some(LogItem::new(raw_log.to_string(), raw_log.to_string()))
    ///     }
    ///     # fn format_preview(&self, _: &LogItem, _: u8) -> String { String::new() }
    ///     # fn get_searchable_text(&self, _: &LogItem, _: u8) -> String { String::new() }
    /// }
    /// ```
    fn parse(&self, raw_log: &str) -> Option<LogItem>;

    /// Formats a log item for display at a given detail level.
    ///
    /// Called frequently during rendering. Returns the string to display in the log list.
    ///
    /// # Detail Level Convention
    ///
    /// Higher levels show more information:
    /// - `0`: Minimal (content only)
    /// - `1`: + timestamp
    /// - `2`: + tag/category
    /// - `3`: + module/component
    /// - `4+`: All fields
    ///
    /// You can define your own levels. Use [`max_detail_level`](LogParser::max_detail_level)
    /// to specify how many levels you support.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use lazylog_framework::{LogParser, LogItem, LogDetailLevel};
    /// # struct MyParser;
    /// impl LogParser for MyParser {
    ///     # fn parse(&self, _: &str) -> Option<LogItem> { None }
    ///     fn format_preview(&self, item: &LogItem, level: LogDetailLevel) -> String {
    ///         match level {
    ///             0 => item.content.clone(),
    ///             1 => format!("{} {}", item.time, item.content),
    ///             _ => format!("{} [{}] {}",
    ///                 item.time,
    ///                 item.get_metadata("level").unwrap_or("INFO"),
    ///                 item.content),
    ///         }
    ///     }
    ///     # fn get_searchable_text(&self, _: &LogItem, _: u8) -> String { String::new() }
    /// }
    /// ```
    fn format_preview(&self, item: &LogItem, detail_level: LogDetailLevel) -> String;

    /// Returns searchable text for filtering at a given detail level.
    ///
    /// When users type `/pattern`, the framework matches against this text.
    /// Include fields that should be searchable at each level.
    ///
    /// # Guidelines
    ///
    /// - At level 0: Just content
    /// - At higher levels: Include more metadata (level, module, etc.)
    /// - Return text that mirrors what's visible in `format_preview`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use lazylog_framework::{LogParser, LogItem, LogDetailLevel};
    /// # struct MyParser;
    /// impl LogParser for MyParser {
    ///     # fn parse(&self, _: &str) -> Option<LogItem> { None }
    ///     # fn format_preview(&self, _: &LogItem, _: u8) -> String { String::new() }
    ///     fn get_searchable_text(&self, item: &LogItem, level: LogDetailLevel) -> String {
    ///         if level >= 2 {
    ///             // at higher levels, include metadata in search
    ///             format!("{} {} {}",
    ///                 item.get_metadata("level").unwrap_or(""),
    ///                 item.get_metadata("module").unwrap_or(""),
    ///                 item.content)
    ///         } else {
    ///             // at low levels, just search content
    ///             item.content.clone()
    ///         }
    ///     }
    /// }
    /// ```
    fn get_searchable_text(&self, item: &LogItem, detail_level: LogDetailLevel) -> String;

    /// Returns text to copy to clipboard when user presses `y`.
    ///
    /// Default: `"{time} {raw_content}"`
    ///
    /// Override to customize clipboard format:
    ///
    /// ```rust
    /// use lazylog_framework::{LogParser, LogItem};
    /// # struct MyParser;
    /// impl LogParser for MyParser {
    ///     # fn parse(&self, _: &str) -> Option<LogItem> { None }
    ///     # fn format_preview(&self, _: &LogItem, _: u8) -> String { String::new() }
    ///     # fn get_searchable_text(&self, _: &LogItem, _: u8) -> String { String::new() }
    ///     fn make_yank_content(&self, item: &LogItem) -> String {
    ///         // copy as JSON
    ///         format!(r#"{{"time": "{}", "message": "{}"}}"#, item.time, item.content)
    ///     }
    /// }
    /// ```
    fn make_yank_content(&self, item: &LogItem) -> String {
        format!("{} {}", item.time, item.raw_content)
    }

    /// Returns the maximum detail level supported by this parser.
    ///
    /// Default: `4` (5 levels: 0-4)
    ///
    /// Override if you support more or fewer levels:
    ///
    /// ```rust
    /// use lazylog_framework::{LogParser, LogDetailLevel};
    /// # struct MyParser;
    /// impl LogParser for MyParser {
    ///     # fn parse(&self, _: &str) -> Option<lazylog_framework::LogItem> { None }
    ///     # fn format_preview(&self, _: &lazylog_framework::LogItem, _: u8) -> String { String::new() }
    ///     # fn get_searchable_text(&self, _: &lazylog_framework::LogItem, _: u8) -> String { String::new() }
    ///     fn max_detail_level(&self) -> LogDetailLevel {
    ///         2  // only 3 levels (0, 1, 2)
    ///     }
    /// }
    /// ```
    fn max_detail_level(&self) -> LogDetailLevel {
        4 // default: 5 levels (0-4)
    }
}

/// Increments detail level (clamped to max).
///
/// # Examples
///
/// ```rust
/// use lazylog_framework::increment_detail_level;
///
/// assert_eq!(increment_detail_level(0, 4), 1);
/// assert_eq!(increment_detail_level(4, 4), 4);  // clamped
/// ```
pub fn increment_detail_level(level: LogDetailLevel, max: LogDetailLevel) -> LogDetailLevel {
    level.saturating_add(1).min(max)
}

/// Decrements detail level (clamped to 0).
///
/// # Examples
///
/// ```rust
/// use lazylog_framework::decrement_detail_level;
///
/// assert_eq!(decrement_detail_level(2), 1);
/// assert_eq!(decrement_detail_level(0), 0);  // clamped
/// ```
pub fn decrement_detail_level(level: LogDetailLevel) -> LogDetailLevel {
    level.saturating_sub(1)
}
