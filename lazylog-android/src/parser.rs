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
        let header = &first_line[1..first_line.len() - 1];

        // find the first '/' to locate LEVEL/TAG
        // format: "MM-DD HH:MM:SS.mmm PID:TID LEVEL/TAG" or "MM-DD HH:MM:SS.mmm PID: TID LEVEL/TAG"
        let slash_pos = match header.find('/') {
            Some(pos) => pos,
            None => {
                // no slash found, malformed header
                return Some(LogItem::new(raw_log.to_string(), raw_log.to_string()));
            }
        };

        // work backwards from slash to find the level (single letter before '/')
        // find the last whitespace before the slash to get the LEVEL/TAG token
        let level_start = header[..slash_pos]
            .rfind(|c: char| c.is_whitespace())
            .map(|pos| pos + 1)
            .unwrap_or(0);

        let level = header[level_start..slash_pos].trim();
        let tag_end = header.len();
        let tag = header[slash_pos + 1..tag_end].trim();
        // if tag is empty after trimming, set to empty string
        let tag = if tag.is_empty() { "" } else { tag };

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
        let origin = item.get_metadata("origin").unwrap_or("");
        let tag = item.get_metadata("tag").unwrap_or("");

        let field_order = [
            ("time", time.as_str()),
            ("level", level),
            ("origin", origin),
            ("tag", tag),
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
                // time + level + origin
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
                // all fields (time + level + origin + tag)
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

    fn max_detail_level(&self) -> LogDetailLevel {
        4 // 5 levels: 0=content, 1=time, 2=time+level, 3=time+level+origin, 4=all
    }
}

/// structured Android log parser - filters for structured logs and delegates to lazylog-parser
pub struct AndroidEffectParser {
    full_parser: AndroidParser,
}

impl AndroidEffectParser {
    pub fn new() -> Self {
        Self {
            full_parser: AndroidParser::new(),
        }
    }
}

impl Default for AndroidEffectParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LogParser for AndroidEffectParser {
    fn parse(&self, raw_log: &str) -> Option<LogItem> {
        // first parse the log to extract tag information
        let parsed = self.full_parser.parse(raw_log)?;

        // filter: only allow logs with "[Effect]" or "CKE-Editor" tag
        let tag = parsed.get_metadata("tag").unwrap_or("");
        let is_allowed_tag = tag == "[Effect]" || tag == "CKE-Editor";

        if !is_allowed_tag {
            // tag not allowed, filter out this log
            return None;
        }

        // check if this log has structured marker
        if !STRUCTURED_MARKER_RE.is_match(raw_log) {
            // no structured marker, filter out this log
            return None;
        }

        // the content from the simple parser is everything after the Android header
        // which should contain the structured log content
        let inner_content = &parsed.content;

        // parse structured content using lazylog-parser
        let log_items = process_delta(inner_content);

        // return first parsed item if available
        if let Some(item) = log_items.into_iter().next() {
            return Some(item);
        }

        // if parsing failed, filter out
        None
    }

    fn format_preview(&self, item: &LogItem, detail_level: LogDetailLevel) -> String {
        self.full_parser.format_preview(item, detail_level)
    }

    fn get_searchable_text(&self, item: &LogItem, detail_level: LogDetailLevel) -> String {
        self.full_parser.get_searchable_text(item, detail_level)
    }

    fn max_detail_level(&self) -> LogDetailLevel {
        self.full_parser.max_detail_level()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_android_log_with_empty_tag() {
        let parser = AndroidParser::new();
        let raw_log = r#"[ 11-14 15:48:35.135 20387:30427 E/         ]
## 2025-11-14 15:48:35 [tid:30427,AMGRichTextParser.cpp:861] error ## [AE_TEXT_TAG]GetLetterRangeFromLetterRange, style 1953785196, 'letterRange' param invalid!"#;

        let result = parser.parse(raw_log);
        assert!(result.is_some());

        let item = result.unwrap();
        assert_eq!(item.get_metadata("level").unwrap(), "E");
        assert_eq!(item.get_metadata("tag").unwrap(), "");
    }

    #[test]
    fn test_parse_android_log_with_effect_tag_error() {
        let parser = AndroidParser::new();
        let raw_log = r#"[ 11-14 15:48:35.135 20387:30427 E/[Effect] ]
## 2025-11-14 15:48:35 [tid:30427,AMGRichTextParser.cpp:861] error ## [AE_TEXT_TAG]GetLetterRangeFromLetterRange, style 1953785196, 'letterRange' param invalid!"#;

        let result = parser.parse(raw_log);
        assert!(result.is_some());

        let item = result.unwrap();
        assert_eq!(item.get_metadata("level").unwrap(), "E");
        assert_eq!(item.get_metadata("tag").unwrap(), "[Effect]");
    }

    #[test]
    fn test_parse_android_log_with_effect_tag_info() {
        let parser = AndroidParser::new();
        let raw_log = r#"[ 11-14 15:48:35.131 20387:30427 I/[Effect] ]
## 2025-11-14 15:48:35 [tid:30427,AMGText.cpp:885] info ## [AE_TEXT_TAG]Set Text bloom path: "#;

        let result = parser.parse(raw_log);
        assert!(result.is_some());

        let item = result.unwrap();
        assert_eq!(item.get_metadata("level").unwrap(), "I");
        assert_eq!(item.get_metadata("tag").unwrap(), "[Effect]");
    }

    #[test]
    fn test_parse_android_log_with_space_after_pid() {
        // new format with space after PID colon: "PID: TID"
        let parser = AndroidParser::new();
        let raw_log = r#"[ 11-14 14:50:22.618  3264: 3264 I/wificond ]
station_bandwidth: "#;

        let result = parser.parse(raw_log);
        assert!(result.is_some());

        let item = result.unwrap();
        assert_eq!(item.get_metadata("level").unwrap(), "I");
        assert_eq!(item.get_metadata("tag").unwrap(), "wificond");
    }

    #[test]
    fn test_parse_android_log_with_tag_containing_colon() {
        // tag contains colon: "unknown:c"
        let parser = AndroidParser::new();
        let raw_log = r#"[ 11-14 15:53:49.156 20387:12953 V/unknown:c ]
Prepared frame frame 15."#;

        let result = parser.parse(raw_log);
        assert!(result.is_some());

        let item = result.unwrap();
        assert_eq!(item.get_metadata("level").unwrap(), "V");
        assert_eq!(item.get_metadata("tag").unwrap(), "unknown:c");
    }

    #[test]
    fn test_parse_android_log_with_tag_with_trailing_spaces() {
        let parser = AndroidParser::new();
        let raw_log = r#"[ 11-14 14:50:28.958  2880: 6202 D/Aurogon  ]
 packageName = com.google.android.gms isAllowWakeUpList "#;

        let result = parser.parse(raw_log);
        assert!(result.is_some());

        let item = result.unwrap();
        assert_eq!(item.get_metadata("level").unwrap(), "D");
        assert_eq!(item.get_metadata("tag").unwrap(), "Aurogon");
    }

    #[test]
    fn test_android_effect_parser_extracts_all_fields() {
        let parser = AndroidEffectParser::new();
        let raw_log = r#"[ 11-14 15:48:35.131 20387:30427 I/[Effect] ]
## 2025-11-14 15:48:35 [tid:30427,AMGText.cpp:885] info ## [AE_TEXT_TAG]Set Text bloom path: "#;

        let result = parser.parse(raw_log);
        assert!(result.is_some());

        let item = result.unwrap();

        // verify all metadata fields are extracted from structured content
        assert_eq!(item.get_metadata("level"), Some("info"));
        assert_eq!(
            item.get_metadata("origin"),
            Some("tid:30427,AMGText.cpp:885")
        );
        assert_eq!(item.get_metadata("tag"), Some("AE_TEXT_TAG"));
        assert_eq!(item.content, "Set Text bloom path:");

        // verify format_preview shows all fields at max detail level
        let preview = parser.format_preview(&item, 4);
        assert!(preview.contains("info"));
        assert!(preview.contains("tid:30427,AMGText.cpp:885"));
        assert!(preview.contains("AE_TEXT_TAG"));
        assert!(preview.contains("Set Text bloom path:"));
    }
}
