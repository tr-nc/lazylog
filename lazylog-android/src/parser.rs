use lazy_static::lazy_static;
use lazylog_framework::provider::{LogDetailLevel, LogItem, LogParser};
use lazylog_parser::process_delta;
use regex::Regex;

lazy_static! {
    // for checking if log contains structured format
    static ref STRUCTURED_MARKER_RE: Regex =
        Regex::new(r"## \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}").unwrap();
}

/// Android logcat parser
pub struct AndroidParser;

impl AndroidParser {
    pub fn new() -> Self {
        Self
    }

    fn shorten_content(content: &str) -> String {
        let lines = content
            .split('\n')
            .map(|line| line.trim())
            .collect::<Vec<&str>>();
        for line in lines {
            if !line.is_empty() {
                return line.to_string();
            }
        }
        content.to_string()
    }
}

impl Default for AndroidParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LogParser for AndroidParser {
    fn parse(&self, raw_log: &str) -> Option<LogItem> {
        // find the first occurrence of ": " to locate the separator
        let colon_space_idx = raw_log.find(": ")?;

        // extract the part before ": "
        let prefix = &raw_log[..colon_space_idx];
        let message = &raw_log[colon_space_idx + 2..];

        // split prefix into tokens
        let tokens: Vec<&str> = prefix.split_whitespace().collect();

        if tokens.len() < 5 {
            // malformed log, return as-is
            return Some(LogItem::new(raw_log.to_string(), raw_log.to_string()));
        }

        // extract components based on Android logcat format:
        // MM-DD HH:MM:SS.mmm  PID  TID LEVEL TAG
        let time = format!("{} {}", tokens[0], tokens[1]);
        let _pid = tokens[2];
        let _tid = tokens[3];
        let level = tokens[4];

        // tag is the last token (previous valid token before ": ")
        let tag = tokens.last().unwrap_or(&"").to_string();

        let mut item = LogItem::new(message.to_string(), raw_log.to_string());
        item.time = time;
        item = item.with_metadata("level", level.to_string());
        item = item.with_metadata("tag", tag);

        Some(item)
    }

    fn format_preview(&self, item: &LogItem, detail_level: LogDetailLevel) -> String {
        let content = Self::shorten_content(&item.content);

        let time = &item.time;
        let level = item.get_metadata("level").unwrap_or("");
        let tag = item.get_metadata("tag").unwrap_or("");

        let field_order = [("time", time.as_str()), ("tag", tag), ("level", level)];

        match detail_level {
            0 => content, // content only
            1 => {
                // time only
                let mut parts = Vec::new();
                for (_, field_value) in field_order.iter().take(1) {
                    if !field_value.is_empty() {
                        parts.push(format!("[{}]", field_value));
                    }
                }
                parts.push(content);
                parts.join(" ")
            }
            2 => {
                // time + tag
                let mut parts = Vec::new();
                for (_, field_value) in field_order.iter().take(2) {
                    if !field_value.is_empty() {
                        parts.push(format!("[{}]", field_value));
                    }
                }
                parts.push(content);
                parts.join(" ")
            }
            _ => {
                // all fields (time + tag + level)
                let mut parts = Vec::new();
                for (_, field_value) in field_order.iter() {
                    if !field_value.is_empty() {
                        parts.push(format!("[{}]", field_value));
                    }
                }
                parts.push(content);
                parts.join(" ")
            }
        }
    }

    fn get_searchable_text(&self, item: &LogItem, detail_level: LogDetailLevel) -> String {
        self.format_preview(item, detail_level)
    }

    fn make_yank_content(&self, item: &LogItem) -> String {
        item.raw_content.clone()
    }

    fn max_detail_level(&self) -> LogDetailLevel {
        3 // 4 levels: 0=content, 1=time, 2=time+tag, 3=all
    }
}

/// structured Android log parser - filters for structured logs and delegates to lazylog-parser
pub struct AndroidEffectParser {
    simple_parser: AndroidParser,
}

impl AndroidEffectParser {
    pub fn new() -> Self {
        Self {
            simple_parser: AndroidParser::new(),
        }
    }

    /// strip Android logcat wrapper to extract inner structured content
    /// Input:  "MM-DD HH:MM:SS.mmm  PID  TID LEVEL TAG: [content...]"
    /// Output: "[content...]"
    fn strip_android_wrapper(android_log: &str) -> Option<&str> {
        android_log.find(": ").map(|idx| &android_log[idx + 2..])
    }
}

impl Default for AndroidEffectParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LogParser for AndroidEffectParser {
    fn parse(&self, raw_log: &str) -> Option<LogItem> {
        // check if this log has structured marker
        if !STRUCTURED_MARKER_RE.is_match(raw_log) {
            // no structured marker, filter out this log
            return None;
        }

        // try to strip Android wrapper
        if let Some(inner_content) = Self::strip_android_wrapper(raw_log) {
            // parse structured content using lazylog-parser
            let log_items = process_delta(inner_content);

            // return first parsed item if available
            if let Some(item) = log_items.into_iter().next() {
                return Some(item);
            }
        }

        // if parsing failed, filter out
        None
    }

    fn format_preview(&self, item: &LogItem, detail_level: LogDetailLevel) -> String {
        self.simple_parser.format_preview(item, detail_level)
    }

    fn get_searchable_text(&self, item: &LogItem, detail_level: LogDetailLevel) -> String {
        self.simple_parser.get_searchable_text(item, detail_level)
    }

    fn make_yank_content(&self, item: &LogItem) -> String {
        self.simple_parser.make_yank_content(item)
    }

    fn max_detail_level(&self) -> LogDetailLevel {
        self.simple_parser.max_detail_level()
    }
}
