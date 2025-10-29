use lazylog_framework::provider::LogItem;

/// Parse iOS syslog format into LogItem
///
/// Format: "Oct 27 16:10:13 deviceName processName[pid] <Level>: content"
/// or:     "Oct 28 19:24:46 backboardd[CoreBrightness](68) <Notice>: content"
pub fn parse_ios_log(raw_log: &str) -> LogItem {
    let parts: Vec<&str> = raw_log.splitn(5, ' ').collect();

    if parts.len() < 5 {
        // malformed log, return as-is
        return LogItem::new(raw_log.to_string(), raw_log.to_string());
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
    item
}
