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
        // -v long format:
        // [ MM-DD HH:MM:SS.mmm  PID: TID LEVEL/TAG ]
        // message line 1
        // message line 2...

        let lines: Vec<&str> = raw_log.lines().collect();
        if lines.is_empty() {
            return None;
        }

        let first_line = lines[0];

        // check if first line starts with '[' and ends with ']'
        if !first_line.starts_with('[') || !first_line.ends_with(']') {
            // malformed log, return as-is
            return Some(LogItem::new(raw_log.to_string(), raw_log.to_string()));
        }

        // extract the header content (without brackets)
        let header = &first_line[1..first_line.len() - 1].trim();

        // split header into tokens
        let tokens: Vec<&str> = header.split_whitespace().collect();

        if tokens.len() < 4 {
            // malformed header, return as-is
            return Some(LogItem::new(raw_log.to_string(), raw_log.to_string()));
        }

        // extract components:
        // tokens[0] = MM-DD
        // tokens[1] = HH:MM:SS.mmm
        // tokens[2] = PID:
        // tokens[3] = TID
        // tokens[4] = LEVEL/TAG

        let _pid = tokens[2].trim_end_matches(':');
        let _tid = tokens[3];

        // level/tag is in format "LEVEL/TAG"
        let level_tag = tokens.get(4).unwrap_or(&"");
        let (level, tag) = if let Some(slash_pos) = level_tag.find('/') {
            let level = &level_tag[..slash_pos];
            let tag = level_tag[slash_pos + 1..].trim();
            // if tag is empty after trimming, set to empty string
            let tag = if tag.is_empty() { "" } else { tag };
            (level, tag)
        } else {
            (*level_tag, "")
        };

        // message is the remaining lines (after the header)
        let message = if lines.len() > 1 {
            lines[1..].join("\n")
        } else {
            String::new()
        };

        // framework generates time automatically
        let item = LogItem::new(message.clone(), raw_log.to_string())
            .with_metadata("level", level.to_string())
            .with_metadata("tag", tag.to_string());

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
