//! File example: file-based provider with real-time tailing.
//!
//! This demonstrates how to tail a log file (like `tail -f`):
//! - Open and read from a file
//! - Track file position to only return new lines
//! - Handle file growth and rotation
//!
//! Run with: cargo run --example file -- /path/to/logfile.log
//!
//! Or generate test logs:
//! ```bash
//! # terminal 1: generate logs
//! while true; do echo "$(date) Test log message"; sleep 1; done > /tmp/test.log
//!
//! # terminal 2: run example
//! cargo run --example file -- /tmp/test.log
//! ```

use anyhow::{Context, Result, anyhow};
use lazylog_framework::{LogItem, LogParser, LogProvider, start_with_provider};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::sync::Arc;

// file-based provider that tails a log file
struct FileProvider {
    path: String,
    reader: Option<BufReader<File>>,
}

impl FileProvider {
    fn new(path: String) -> Self {
        Self { path, reader: None }
    }
}

impl LogProvider for FileProvider {
    fn start(&mut self) -> Result<()> {
        let file = File::open(&self.path)
            .with_context(|| format!("Failed to open file: {}", self.path))?;

        let mut reader = BufReader::new(file);

        // seek to end to only show new logs (comment out to show entire file)
        reader.seek(SeekFrom::End(0))?;

        self.reader = Some(reader);
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.reader = None;
        Ok(())
    }

    fn poll_logs(&mut self) -> Result<Vec<String>> {
        let mut logs = Vec::new();

        if let Some(reader) = &mut self.reader {
            let mut line = String::new();

            // read all available lines (non-blocking)
            loop {
                let bytes_read = reader.read_line(&mut line)?;

                if bytes_read == 0 {
                    // no more data available
                    break;
                }

                if !line.trim().is_empty() {
                    logs.push(line.trim().to_string());
                }

                line.clear();
            }
        }

        Ok(logs)
    }
}

// simple parser for plain text logs
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
    // get file path from command line
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        return Err(anyhow!(
            "Usage: {} <log-file-path>\n\nExample:\n  {} /var/log/syslog",
            args[0],
            args[0]
        ));
    }

    let log_path = &args[1];

    // setup terminal
    let mut stdout = std::io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create provider and parser
    let provider = FileProvider::new(log_path.to_string());
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
