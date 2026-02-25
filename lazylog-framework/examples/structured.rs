//! Structured example: JSON log parsing with detail levels.
//!
//! This demonstrates advanced parsing features:
//! - Parse JSON logs and extract fields
//! - Use metadata to store structured data
//! - Implement multiple detail levels
//! - Filter logs by level
//!
//! Run with: cargo run --example structured

use anyhow::Result;
use lazylog_framework::{
    AppDesc, LogDetailLevel, LogItem, LogParser, LogProvider, start_with_desc,
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::Arc;
use std::time::Duration;

// provider that generates structured JSON logs
struct JsonLogProvider {
    counter: usize,
}

impl JsonLogProvider {
    fn new() -> Self {
        Self { counter: 0 }
    }
}

impl LogProvider for JsonLogProvider {
    fn start(&mut self) -> Result<()> {
        println!("JSON provider started. Generating structured logs...");
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        println!("JSON provider stopped.");
        Ok(())
    }

    fn poll_logs(&mut self) -> Result<Vec<String>> {
        self.counter += 1;

        // generate different log levels
        let level = match self.counter % 4 {
            0 => "ERROR",
            1 => "WARN",
            2 => "INFO",
            _ => "DEBUG",
        };

        let module = match self.counter % 3 {
            0 => "auth",
            1 => "database",
            _ => "api",
        };

        // generate JSON log
        let log = format!(
            r#"{{"timestamp":"2025-01-15T10:30:00Z","level":"{}","module":"{}","message":"Log message #{}","request_id":"req-{:04}"}}"#,
            level, module, self.counter, self.counter
        );

        // slow down generation
        std::thread::sleep(Duration::from_millis(500));

        Ok(vec![log])
    }
}

// parser for structured JSON logs
struct JsonParser;

impl LogParser for JsonParser {
    fn parse(&self, raw_log: &str) -> Option<LogItem> {
        // parse JSON
        let json: serde_json::Value = serde_json::from_str(raw_log).ok()?;

        // extract fields
        let message = json["message"].as_str()?.to_string();
        let level = json["level"].as_str().unwrap_or("INFO");
        let module = json["module"].as_str().unwrap_or("unknown");
        let request_id = json["request_id"].as_str().unwrap_or("");

        // filter out DEBUG logs (optional)
        // if level == "DEBUG" {
        //     return None;
        // }

        // create log item with metadata
        Some(
            LogItem::new(message, raw_log.to_string())
                .with_metadata("level", level)
                .with_metadata("module", module)
                .with_metadata("request_id", request_id),
        )
    }

    fn format_preview(&self, item: &LogItem, detail_level: LogDetailLevel) -> String {
        let level = item.get_metadata("level").unwrap_or("INFO");
        let module = item.get_metadata("module").unwrap_or("?");
        let request_id = item.get_metadata("request_id").unwrap_or("");

        match detail_level {
            0 => {
                // level 0: content only
                item.content.clone()
            }
            1 => {
                // level 1: time + content
                format!("[{}] {}", item.time, item.content)
            }
            2 => {
                // level 2: time + level + content
                format!("[{}] {:5} {}", item.time, level, item.content)
            }
            3 => {
                // level 3: time + level + module + content
                format!("[{}] {:5} [{}] {}", item.time, level, module, item.content)
            }
            _ => {
                // level 4+: all fields
                format!(
                    "[{}] {:5} [{}] [{}] {}",
                    item.time, level, module, request_id, item.content
                )
            }
        }
    }

    fn get_searchable_text(&self, item: &LogItem, detail_level: LogDetailLevel) -> String {
        // include more fields at higher detail levels
        match detail_level {
            0 => item.content.clone(),
            1 => format!("{} {}", item.time, item.content),
            2 => format!(
                "{} {} {}",
                item.time,
                item.get_metadata("level").unwrap_or(""),
                item.content
            ),
            _ => format!(
                "{} {} {} {}",
                item.time,
                item.get_metadata("level").unwrap_or(""),
                item.get_metadata("module").unwrap_or(""),
                item.content
            ),
        }
    }

    fn max_detail_level(&self) -> LogDetailLevel {
        4 // support 5 levels (0-4)
    }
}

fn main() -> Result<()> {
    // setup terminal
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create provider and parser
    let provider = JsonLogProvider::new();
    let parser = Arc::new(JsonParser);

    // configure with custom settings
    let mut desc = AppDesc::new(parser);
    desc.poll_interval = Duration::from_millis(100); // poll every 100ms
    desc.ring_buffer_size = 10000; // 10K log capacity

    // run the application
    let result = start_with_desc(&mut terminal, provider, desc);

    // restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}
