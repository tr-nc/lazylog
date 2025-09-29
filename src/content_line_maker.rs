use ratatui::text::Line;

pub enum WrappingMode {
    Wrapped,
    Unwrapped,
    Truncated,
}

pub fn content_into_lines(content: &str, width: u16, wrapping_mode: WrappingMode) -> Vec<Line<'_>> {
    //     content.lines().map(Line::from).collect()

    match wrapping_mode {
        WrappingMode::Wrapped => wrap_content_to_lines(content, width),
        WrappingMode::Unwrapped => content_to_unwrapped_lines(content),
        WrappingMode::Truncated => vec![truncate_content(content, width)],
    }
}

pub fn calculate_content_width(content: &str) -> usize {
    content
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
}

fn content_to_a_single_line(content: &str) -> Line<'_> {
    Line::from(content)
}

fn content_to_unwrapped_lines(content: &str) -> Vec<Line<'_>> {
    content.lines().map(Line::from).collect()
}

fn truncate_content(content: &str, width: u16) -> Line<'_> {
    if width == 0 {
        return Line::from("");
    }

    let width = width as usize;
    let first_line = content.lines().next().unwrap_or("");

    if first_line.chars().count() <= width {
        Line::from(first_line)
    } else {
        let truncated: String = first_line.chars().take(width.saturating_sub(2)).collect();
        Line::from(format!("{}..", truncated))
    }
}

fn wrap_content_to_lines(content: &str, width: u16) -> Vec<Line<'_>> {
    if width == 0 {
        return vec![];
    }

    let width = width as usize;
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for ch in content.chars() {
        if ch == '\n' {
            lines.push(Line::from(current_line.clone()));
            current_line.clear();
        } else {
            current_line.push(ch);
            if current_line.len() == width {
                lines.push(Line::from(current_line.clone()));
                current_line.clear();
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_content() {
        let result = wrap_content_to_lines("", 10);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_zero_width() {
        let result = wrap_content_to_lines("hello", 0);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_short_content() {
        let result = wrap_content_to_lines("hello", 10);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to_string(), "hello");
    }

    #[test]
    fn test_exact_width() {
        let result = wrap_content_to_lines("hello", 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to_string(), "hello");
    }

    #[test]
    fn test_long_content() {
        let result = wrap_content_to_lines("hello world", 5);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].to_string(), "hello");
        assert_eq!(result[1].to_string(), " worl");
        assert_eq!(result[2].to_string(), "d");
    }

    #[test]
    fn test_newline_handling() {
        let result = wrap_content_to_lines("hello\nworld", 10);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].to_string(), "hello");
        assert_eq!(result[1].to_string(), "world");
    }

    #[test]
    fn test_multiple_newlines() {
        let result = wrap_content_to_lines("hello\n\nworld", 10);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].to_string(), "hello");
        assert_eq!(result[1].to_string(), "");
        assert_eq!(result[2].to_string(), "world");
    }

    #[test]
    fn test_very_long_content() {
        let result = wrap_content_to_lines("this is a very long line that needs to be wrapped", 10);
        assert_eq!(result.len(), 5);
        assert_eq!(result[0].to_string(), "this is a ");
        assert_eq!(result[1].to_string(), "very long ");
        assert_eq!(result[2].to_string(), "line that ");
        assert_eq!(result[3].to_string(), "needs to b");
        assert_eq!(result[4].to_string(), "e wrapped");
    }

    #[test]
    fn test_arrange_content_wrapped() {
        let result = content_into_lines("hello world", 5, WrappingMode::Wrapped);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].to_string(), "hello");
        assert_eq!(result[1].to_string(), " worl");
        assert_eq!(result[2].to_string(), "d");
    }

    #[test]
    fn test_arrange_content_unwrapped() {
        let result = content_into_lines("hello world\nsecond line", 5, WrappingMode::Unwrapped);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].to_string(), "hello world");
        assert_eq!(result[1].to_string(), "second line");
    }

    #[test]
    fn test_arrange_content_unwrapped_with_long_lines() {
        let result = content_into_lines(
            "this is a very long line that exceeds width\nshort",
            10,
            WrappingMode::Unwrapped,
        );
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0].to_string(),
            "this is a very long line that exceeds width"
        );
        assert_eq!(result[1].to_string(), "short");
    }

    #[test]
    fn test_arrange_content_truncated() {
        let result = content_into_lines("hello world", 5, WrappingMode::Truncated);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to_string(), "hel..");
    }

    #[test]
    fn test_arrange_content_truncated_short() {
        let result = content_into_lines("hi", 5, WrappingMode::Truncated);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to_string(), "hi");
    }

    #[test]
    fn test_arrange_content_truncated_exact() {
        let result = content_into_lines("hello", 5, WrappingMode::Truncated);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to_string(), "hello");
    }

    #[test]
    fn test_arrange_content_truncated_multiline() {
        let result = content_into_lines("hello world\nsecond line", 5, WrappingMode::Truncated);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to_string(), "hel..");
    }

    #[test]
    fn test_arrange_content_truncated_zero_width() {
        let result = content_into_lines("hello", 0, WrappingMode::Truncated);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to_string(), "");
    }
}
