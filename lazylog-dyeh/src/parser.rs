use lazy_static::lazy_static;
use lazylog_framework::provider::LogItem;
use regex::Regex;

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
        LogItem::new(raw_content.clone(), raw_content)
    })
}

pub fn process_delta(delta: &str) -> Vec<LogItem> {
    let body = remove_inline_headers(strip_leading_header(delta))
        .trim()
        .to_string();
    if body.is_empty() {
        return Vec::new();
    }

    let mut starts: Vec<usize> = ITEM_SEP_RE.find_iter(&body).map(|m| m.start()).collect();

    if starts.is_empty() {
        return Vec::new();
    }

    let mut items = Vec::new();
    starts.push(body.len()); // sentinel
    for win in starts.windows(2) {
        if let [s, e] = *win
            && let Some(it) = parse_structured(&body[s..e])
        {
            let (origin, level, tag, msg) = split_header(&it.content);
            let mut updated_item = LogItem::new(msg, it.raw_content);

            // add metadata if fields are not empty
            if !level.is_empty() {
                updated_item = updated_item.with_metadata("level", level);
            }
            if !origin.is_empty() {
                updated_item = updated_item.with_metadata("origin", origin);
            }
            if !tag.is_empty() {
                updated_item = updated_item.with_metadata("tag", tag);
            }

            items.push(updated_item);
        }
    }

    items
}
