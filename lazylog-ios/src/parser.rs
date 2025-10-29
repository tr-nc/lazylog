use lazy_static::lazy_static;
use lazylog_framework::provider::{LogDetailLevel, LogItem, LogParser};
use lazylog_parser::process_delta;
use regex::Regex;

lazy_static! {
    // for checking if log contains structured format
    static ref STRUCTURED_MARKER_RE: Regex =
        Regex::new(r"## \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}").unwrap();
}

/// simple iOS log parser - parses basic iOS syslog format
pub struct IosFullParser;

impl IosFullParser {
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

impl Default for IosFullParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LogParser for IosFullParser {
    fn parse(&self, raw_log: &str) -> Option<LogItem> {
        let parts: Vec<&str> = raw_log.splitn(5, ' ').collect();

        if parts.len() < 5 {
            // malformed log, return as-is
            return Some(LogItem::new(raw_log.to_string(), raw_log.to_string()));
        }

        // extract tag: the 4th item (index 3), process it to only leave the name before [ or (
        let tag = parts[3]
            .split('[')
            .next()
            .and_then(|s| s.split('(').next())
            .unwrap_or(parts[3])
            .to_string();

        // level and content from the 5th item onwards
        let level_and_content = parts[4];
        let (level, content) = if let Some(start) = level_and_content.find('<') {
            if let Some(end) = level_and_content.find(">:") {
                // extract level without angle brackets
                let level = &level_and_content[start + 1..end];
                let content = &level_and_content[end + 2..];
                (level.to_string(), content.trim().to_string())
            } else {
                (String::new(), level_and_content.to_string())
            }
        } else {
            (String::new(), level_and_content.to_string())
        };

        let mut item = LogItem::new(content, raw_log.to_string());
        if !level.is_empty() {
            item = item.with_metadata("level", level);
        }
        if !tag.is_empty() {
            item = item.with_metadata("tag", tag);
        }
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

/// structured iOS log parser - filters for structured logs and delegates to lazylog-parser
pub struct IosEffectParser {
    simple_parser: IosFullParser,
}

impl IosEffectParser {
    pub fn new() -> Self {
        Self {
            simple_parser: IosFullParser::new(),
        }
    }

    /// strip iOS wrapper to extract inner structured content
    /// Input:  "Oct 29 11:27:36 EffectCam[6923] <Notice>: [content...]"
    /// Output: "[content...]"
    fn strip_ios_wrapper(ios_log: &str) -> Option<&str> {
        ios_log.find(">: ").map(|idx| &ios_log[idx + 3..])
    }
}

impl Default for IosEffectParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LogParser for IosEffectParser {
    fn parse(&self, raw_log: &str) -> Option<LogItem> {
        // check if this log has structured marker
        if !STRUCTURED_MARKER_RE.is_match(raw_log) {
            // no structured marker, filter out this log
            return None;
        }

        // try to strip iOS wrapper
        if let Some(inner_content) = Self::strip_ios_wrapper(raw_log) {
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
