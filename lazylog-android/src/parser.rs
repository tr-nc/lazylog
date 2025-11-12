use lazylog_framework::provider::{LogDetailLevel, LogItem, LogParser};

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
