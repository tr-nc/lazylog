//! Simple example: minimal provider that generates dummy logs.
//!
//! This demonstrates the bare minimum needed to create a log viewer:
//! - Implement LogProvider to generate logs
//! - Implement LogParser to format logs
//! - Call start_with_provider() to launch the TUI
//!
//! Run with: cargo run --example simple

use anyhow::Result;
use lazylog_framework::{LogItem, LogParser, LogProvider, start_with_provider};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::Arc;

// simple provider that generates dummy logs
struct DummyProvider {
    counter: usize,
}

impl DummyProvider {
    fn new() -> Self {
        Self { counter: 0 }
    }
}

impl LogProvider for DummyProvider {
    fn start(&mut self) -> Result<()> {
        println!("Dummy provider started. Generating logs...");
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        println!("Dummy provider stopped.");
        Ok(())
    }

    fn poll_logs(&mut self) -> Result<Vec<String>> {
        // generate one log per poll
        self.counter += 1;
        let log = format!("Log message #{}", self.counter);

        // slow down generation to make it visible
        std::thread::sleep(std::time::Duration::from_millis(500));

        Ok(vec![log])
    }
}

// simple parser that displays logs as-is
struct PlainParser;

impl LogParser for PlainParser {
    fn parse(&self, raw_log: &str) -> Option<LogItem> {
        Some(LogItem::new(raw_log.to_string(), raw_log.to_string()))
    }

    fn format_preview(&self, item: &LogItem, detail_level: u8) -> String {
        match detail_level {
            0 => item.content.clone(),
            _ => format!("[{}] {}", item.time, item.content),
        }
    }

    fn get_searchable_text(&self, item: &LogItem, _detail_level: u8) -> String {
        item.content.clone()
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
    let provider = DummyProvider::new();
    let parser = Arc::new(PlainParser);

    // run the application
    let result = start_with_provider(&mut terminal, provider, parser);

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
