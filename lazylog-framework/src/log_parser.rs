// NOTE: this file is kept for backward compatibility with app.rs
// LogItem and LogDetailLevel are now defined in provider::log_item
// DYEH-specific parsing has been moved to dyeh::parser

pub use crate::provider::{LogDetailLevel, LogItem};

impl LogItem {
    pub fn make_yank_content(&self) -> String {
        self.raw_content.clone()
    }

    pub fn contains(&self, pattern: &str, detail_level: LogDetailLevel) -> bool {
        self.get_preview_text(detail_level)
            .to_lowercase()
            .contains(&pattern.to_lowercase())
    }

    pub fn get_preview_text(&self, detail_level: LogDetailLevel) -> String {
        let content = shorten_content(&self.content);
        self.format_with_fields(detail_level, &content)
    }

    fn format_with_fields(&self, detail_level: LogDetailLevel, content: &str) -> String {
        let field_order = [
            ("time", &self.time),
            ("tag", &self.tag),
            ("origin", &self.origin),
            ("level", &self.level),
        ];

        match detail_level {
            LogDetailLevel::ContentOnly => content.to_string(),
            LogDetailLevel::Basic => {
                let mut parts = Vec::new();
                if let Some((_, field_value)) = field_order.first()
                    && !field_value.is_empty()
                {
                    parts.push(format!("[{}]", field_value));
                }
                parts.push(content.to_string());
                parts.join(" ")
            }
            LogDetailLevel::Medium => {
                let mut parts = Vec::new();
                for (_, field_value) in field_order.iter().take(2) {
                    if !field_value.is_empty() {
                        parts.push(format!("[{}]", field_value));
                    }
                }
                parts.push(content.to_string());
                parts.join(" ")
            }
            LogDetailLevel::Detailed => {
                let mut parts = Vec::new();
                for (_, field_value) in field_order.iter().take(3) {
                    if !field_value.is_empty() {
                        parts.push(format!("[{}]", field_value));
                    }
                }
                parts.push(content.to_string());
                parts.join(" ")
            }
            LogDetailLevel::Full => {
                let mut parts = Vec::new();
                for (_, field_value) in field_order.iter() {
                    if !field_value.is_empty() {
                        parts.push(format!("[{}]", field_value));
                    }
                }
                parts.push(content.to_string());
                parts.join(" ")
            }
        }
    }
}

impl LogDetailLevel {
    pub fn increment(&self) -> Self {
        self.cycle_forward()
    }

    pub fn decrement(&self) -> Self {
        self.cycle_backward()
    }
}

impl Default for LogDetailLevel {
    fn default() -> Self {
        Self::Basic
    }
}

/// split the content by \n, trim each item, and find the first trimmed item that is not empty
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
