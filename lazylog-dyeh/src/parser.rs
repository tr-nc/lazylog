use lazylog_framework::provider::{LogDetailLevel, LogItem, LogParser};
use lazylog_parser::process_delta;

/// DYEH log parser - uses lazylog-parser to parse structured DYEH logs
pub struct DyehParser;

impl DyehParser {
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

impl Default for DyehParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LogParser for DyehParser {
    fn parse(&self, raw_log: &str) -> Option<LogItem> {
        // use lazylog-parser to parse structured DYEH log format
        let log_items = process_delta(raw_log);

        // return first parsed item if available, otherwise create a simple item
        Some(
            log_items
                .into_iter()
                .next()
                .unwrap_or_else(|| LogItem::new(raw_log.to_string(), raw_log.to_string())),
        )
    }

    fn format_preview(&self, item: &LogItem, detail_level: LogDetailLevel) -> String {
        let content = Self::shorten_content(&item.content);

        let time = &item.time;
        let level = item.get_metadata("level").unwrap_or("");
        let tag = item.get_metadata("tag").unwrap_or("");
        let origin = item.get_metadata("origin").unwrap_or("");

        let field_order = [
            ("time", time.as_str()),
            ("level", level),
            ("tag", tag),
            ("origin", origin),
        ];

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
                // time + level
                let mut parts = Vec::new();
                for (_, field_value) in field_order.iter().take(2) {
                    if !field_value.is_empty() {
                        parts.push(format!("[{}]", field_value));
                    }
                }
                parts.push(content);
                parts.join(" ")
            }
            3 => {
                // time + level + tag
                let mut parts = Vec::new();
                for (_, field_value) in field_order.iter().take(3) {
                    if !field_value.is_empty() {
                        parts.push(format!("[{}]", field_value));
                    }
                }
                parts.push(content);
                parts.join(" ")
            }
            _ => {
                // all fields (time + level + tag + origin)
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
        4 // 5 levels: 0=content, 1=time, 2=time+level, 3=time+level+tag, 4=all
    }
}
