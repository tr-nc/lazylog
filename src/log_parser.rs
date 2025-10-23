use lazy_static::lazy_static;
use regex::Regex;
use std::ops::Range;
use uuid::Uuid;

lazy_static! {
    static ref LEADING_HEADER_RE: Regex = Regex::new(
        r"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}\] \[\w+\]\s*\n?"
    ).unwrap();

    static ref INLINE_HEADER_RE: Regex = Regex::new(
        r"\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}\] \[\w+\]\s*"
    ).unwrap();

    static ref ITEM_SEP_RE: Regex =
        Regex::new(r"## \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}").unwrap();

    static ref ITEM_PARSE_RE: Regex =
        Regex::new(r"(?s)^## (\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\s*(.*)").unwrap();

    // extracts:  [origin] LEVEL ## [TAG] message…
    // important: In (?x) mode, `#` starts a comment. Escape the hashes as \#\#.
    static ref CONTENT_HEADER_RE: Regex = Regex::new(
        r"(?xs)
          ^\[(?P<origin>[^\]]+)]\s*
          (?P<level>[A-Z]+)\s*
          \#\#\s*
          \[(?P<tag>[^\]]+)]\s*
          (?P<msg>.*)"
    ).unwrap();
}

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
    pub fn make_yank_content(&self) -> String {
        format!(
            "# Formatted Log\n\n## Time:\n\n{}\n\n## Level:\n\n{}\n\n## Origin:\n\n{}\n\n## Tag:\n\n{}\n\n## Content:\n\n{}\n\n# Raw Log\n\n{}",
            self.time, self.level, self.origin, self.tag, self.content, self.raw_content
        )
    }

    pub fn contains(&self, pattern: &str, detail_level: u8) -> bool {
        self.get_preview_text(detail_level)
            .to_lowercase()
            .contains(&pattern.to_lowercase())
    }

    pub fn get_preview_text(&self, detail_level: u8) -> String {
        let content = shorten_content(&self.content);

        let base_format = self.format_with_fields(detail_level, &content);

        return base_format;

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
    }

    fn format_with_fields(&self, detail_level: u8, content: &str) -> String {
        let field_order = [
            ("time", &self.time),
            ("tag", &self.tag),
            ("origin", &self.origin),
            ("level", &self.level),
        ];

        match detail_level {
            0 => content.to_string(),
            1 => {
                let mut parts = Vec::new();
                if let Some((_, field_value)) = field_order.first()
                    && !field_value.is_empty()
                {
                    parts.push(format!("[{}]", field_value));
                }
                parts.push(content.to_string());
                parts.join(" ")
            }
            2 => {
                let mut parts = Vec::new();
                for (_, field_value) in field_order.iter().take(2) {
                    if !field_value.is_empty() {
                        parts.push(format!("[{}]", field_value));
                    }
                }
                parts.push(content.to_string());
                parts.join(" ")
            }
            3 => {
                let mut parts = Vec::new();
                for (_, field_value) in field_order.iter().take(3) {
                    if !field_value.is_empty() {
                        parts.push(format!("[{}]", field_value));
                    }
                }
                parts.push(content.to_string());
                parts.join(" ")
            }
            4 => {
                let mut parts = Vec::new();
                for (_, field_value) in field_order.iter() {
                    if !field_value.is_empty() {
                        parts.push(format!("[{}]", field_value));
                    }
                }
                parts.push(content.to_string());
                parts.join(" ")
            }
            _ => {
                let mut parts = Vec::new();
                if let Some((_, field_value)) = field_order.first()
                    && !field_value.is_empty()
                {
                    parts.push(format!("[{}]", field_value));
                }
                parts.push(content.to_string());
                parts.join(" ")
            }
        }
    }
}

/* ───────────────────── special-event framework ────────────────────────── */
mod special_events {
    use super::*;

    pub struct MatchedEvent {
        pub span: Range<usize>,
        pub item: LogItem,
    }

    pub trait EventMatcher: Sync + Send {
        fn capture(&self, text: &str) -> Vec<MatchedEvent>;
    }

    /* ------------------------------- Pause ------------------------------ */
    struct PauseMatcher;

    impl PauseMatcher {
        fn pause_block_ranges(text: &str) -> Vec<Range<usize>> {
            lazy_static! {
                static ref PAUSE_RE: Regex =
                    Regex::new(r"(?i)bef_effect_onpause_imp\s*\(|onpause").unwrap();
            }
            let mut ranges: Vec<Range<usize>> = PAUSE_RE
                .find_iter(text)
                .map(|m| {
                    let mut s = m.start();
                    let mut e = m.end();
                    s = text[..s].rfind('\n').map_or(0, |p| p + 1);
                    e += text[e..].find('\n').map_or(text.len() - e, |p| p + 1);
                    s..e
                })
                .collect();
            ranges.sort_by_key(|r| r.start);
            let mut merged = Vec::<Range<usize>>::new();
            for r in ranges {
                if let Some(last) = merged.last_mut()
                    && r.start <= last.end + 1
                {
                    last.end = last.end.max(r.end);
                    continue;
                }
                merged.push(r.clone());
            }
            merged
        }
    }

    impl EventMatcher for PauseMatcher {
        fn capture(&self, text: &str) -> Vec<MatchedEvent> {
            Self::pause_block_ranges(text)
                .into_iter()
                .map(|span| MatchedEvent {
                    span,
                    item: LogItem {
                        id: Uuid::new_v4(),
                        time: String::new(),
                        origin: String::new(),
                        level: String::new(),
                        tag: String::new(),
                        content: "DYEH PAUSED".to_string(),
                        raw_content: "DYEH PAUSED".to_string(),
                    },
                })
                .collect()
        }
    }

    struct ResumeMatcher;

    impl ResumeMatcher {
        fn resume_block_ranges(text: &str) -> Vec<Range<usize>> {
            lazy_static! {
                static ref RESUME_RE: Regex =
                    Regex::new(r"(?i)bef_effect_onresume_imp\s*\(").unwrap();
            }
            let mut ranges: Vec<Range<usize>> = RESUME_RE
                .find_iter(text)
                .map(|m| {
                    let mut s = m.start();
                    let mut e = m.end();
                    s = text[..s].rfind('\n').map_or(0, |p| p + 1);
                    e += text[e..].find('\n').map_or(text.len() - e, |p| p + 1);
                    s..e
                })
                .collect();
            ranges.sort_by_key(|r| r.start);
            let mut merged = Vec::<Range<usize>>::new();
            for r in ranges {
                if let Some(last) = merged.last_mut()
                    && r.start <= last.end + 1
                {
                    last.end = last.end.max(r.end);
                    continue;
                }
                merged.push(r.clone());
            }
            merged
        }
    }

    impl EventMatcher for ResumeMatcher {
        fn capture(&self, text: &str) -> Vec<MatchedEvent> {
            Self::resume_block_ranges(text)
                .into_iter()
                .map(|span| MatchedEvent {
                    span,
                    item: LogItem {
                        id: Uuid::new_v4(),
                        time: String::new(),
                        origin: String::new(),
                        level: String::new(),
                        tag: String::new(),
                        content: "DYEH RESUMED".to_string(),
                        raw_content: "DYEH RESUMED".to_string(),
                    },
                })
                .collect()
        }
    }

    lazy_static! {
        pub static ref MATCHERS: Vec<Box<dyn EventMatcher>> =
            vec![Box::new(PauseMatcher), Box::new(ResumeMatcher)];
    }
}
use special_events::{MATCHERS, MatchedEvent};

fn strip_leading_header(s: &str) -> &str {
    LEADING_HEADER_RE
        .find(s)
        .map(|m| &s[m.end()..])
        .unwrap_or(s)
}

fn remove_inline_headers(s: &str) -> String {
    INLINE_HEADER_RE.replace_all(s, "").into_owned()
}

// split "[origin] LEVEL ## [TAG] …" → (origin, level, tag, msg)
fn split_header(line: &str) -> (String, String, String, String) {
    // be robust to BOM/control chars that might precede the first "[".
    let line =
        line.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c.is_control());

    if let Some(caps) = CONTENT_HEADER_RE.captures(line) {
        (
            caps["origin"].trim().to_owned(),
            caps["level"].trim().to_owned(),
            caps["tag"].trim().to_owned(),
            caps["msg"].trim().to_owned(),
        )
    } else {
        (
            String::new(),
            String::new(),
            String::new(),
            line.trim().to_owned(),
        )
    }
}

fn parse_structured(block: &str) -> Option<LogItem> {
    ITEM_PARSE_RE.captures(block).map(|caps| {
        let raw_content = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
        LogItem {
            id: Uuid::new_v4(),
            time: caps.get(1).map_or("", |m| m.as_str()).to_string(),
            origin: String::new(),
            level: String::new(),
            tag: String::new(),
            content: raw_content.clone(),
            raw_content,
        }
    })
}

/* ─────────────────────────────── API ──────────────────────────────────── */
pub fn process_delta(delta: &str) -> Vec<LogItem> {
    /* 1 ── initial cleaning --------------------------------------------- */
    let body = remove_inline_headers(strip_leading_header(delta))
        .trim()
        .to_string();
    if body.is_empty() {
        return Vec::new();
    }

    /* 2 ── collect *positioned* special events -------------------------- */
    let mut positioned: Vec<(usize, LogItem)> = Vec::new();
    for matcher in MATCHERS.iter() {
        for MatchedEvent { span, item } in matcher.capture(&body) {
            positioned.push((span.start, item));
        }
    }

    /* 3 ── parse the regular “## …” items ------------------------------- */
    let mut starts: Vec<usize> = ITEM_SEP_RE.find_iter(&body).map(|m| m.start()).collect();

    if !starts.is_empty() {
        starts.push(body.len()); // sentinel
        for win in starts.windows(2) {
            if let [s, e] = *win
                && let Some(mut it) = parse_structured(&body[s..e])
            {
                let (o, l, t, msg) = split_header(&it.content);
                it.origin = o;
                it.level = l;
                it.tag = t;
                it.content = msg;
                positioned.push((s, it));
            }
        }
    }

    /* 4 ── restore the natural order ------------------------------------ */
    positioned.sort_by_key(|(pos, _)| *pos);

    /* 5 ── just return them – no collapsing ----------------------------- */
    positioned.into_iter().map(|(_, it)| it).collect()
}
