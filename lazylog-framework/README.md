# lazylog-framework

A powerful, extensible framework for building terminal-based log viewers with vim-like navigation and real-time monitoring capabilities.

## Features

- **Provider-based architecture** - Pluggable log sources
- **Vim-like navigation** - j/k, gg/G, Ctrl+d/u, and more
- **Real-time streaming** - Monitor logs as they arrive
- **Filtering** - Dynamic log filtering with `/` search
- **Detail levels** - Control information density (0-4)
- **Mouse support** - Click and scroll
- **Modern UI** - Clean interface with tailwind-inspired colors
- **Clipboard integration** - Yank logs with `y`
- **Memory-efficient** - Ring buffer prevents unbounded growth

## Installation

```toml
[dependencies]
lazylog-framework = "0.3"
```

## Quick Start

```rust
use lazylog_framework::{LogProvider, LogParser, LogItem, start_with_provider};
use anyhow::Result;
use std::sync::Arc;

// 1. implement LogProvider for your log source
struct MyLogProvider;

impl LogProvider for MyLogProvider {
    fn start(&mut self) -> Result<()> {
        // setup resources (open files, connect to streams, etc.)
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        // cleanup resources
        Ok(())
    }

    fn poll_logs(&mut self) -> Result<Vec<String>> {
        // return raw log strings since last poll (non-blocking)
        Ok(vec!["2025-01-15 10:30:00 INFO Application started".to_string()])
    }
}

// 2. implement LogParser to format your logs
struct MyLogParser;

impl LogParser for MyLogParser {
    fn parse(&self, raw_log: &str) -> Option<LogItem> {
        // parse raw string into LogItem
        Some(LogItem::new(raw_log.to_string(), raw_log.to_string()))
    }

    fn format_preview(&self, item: &LogItem, detail_level: u8) -> String {
        // format log for display at given detail level
        match detail_level {
            0 => item.content.clone(),
            _ => format!("[{}] {}", item.time, item.content),
        }
    }

    fn get_searchable_text(&self, item: &LogItem, _detail_level: u8) -> String {
        // return text that should be searchable
        item.content.clone()
    }
}

// 3. run the application
fn main() -> Result<()> {
    use ratatui::{Terminal, backend::CrosstermBackend};
    use std::io;

    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let provider = MyLogProvider;
    let parser = Arc::new(MyLogParser);

    lazylog_framework::start_with_provider(&mut terminal, provider, parser)?;
    Ok(())
}
```

## Architecture

The framework uses a two-trait system that separates log acquisition from parsing:

```
┌──────────────┐   poll_logs()      ┌─────────────┐
│ LogProvider  │ ─────────────────> │ Vec<String> │ (raw logs)
└──────────────┘                    └──────┬──────┘
                                           │
                                           │ parse()
                                           │
┌──────────────┐  format_preview()  ┌──────▼─────┐
│  LogParser   │ <───────────────── │  LogItem   │
└──────────────┘                    └────────────┘
         │                                 │
         └──────────────┬──────────────────┘
                        │
                        ↓
              ┌──────────────────┐
              │   Ring Buffer    │
              │   (16K capacity) │
              └─────────┬────────┘
                        │
                        ↓
              ┌──────────────────┐
              │ lazylog-framework│
              │  - Rendering     │
              │  - Navigation    │
              │  - Filtering     │
              │  - UI management │
              └─────────┬────────┘
                        │
                        ↓
                  ┌──────────┐
                  │ Terminal │
                   └──────────┘
```

## Performance & Responsiveness

Lazylog-framework prioritizes snappy user interaction:

### Event Loop Architecture

```
┌─────────────────────────────────────────────────┐
│              Main Application Loop              │
├─────────────────────────────────────────────────┤
│  Event Poll (16ms)                             │
│  ├─ Keyboard input (~60fps response)           │
│  ├─ Mouse events                               │
│  └─ Terminal resize                           │
├─────────────────────────────────────────────────┤
│  Provider Poll (100ms, throttled)             │
│  └─ Log ingestion from ring buffer            │
├─────────────────────────────────────────────────┤
│  Render (after each event or provider poll)    │
│  └─ UI update at 16ms intervals              │
└─────────────────────────────────────────────────┘
```

### Key Design Principles

- **Separate intervals**: Events at 16ms, providers at 100ms
- **Interruptible sleeps**: Provider thread checks stop signal every 25ms
- **Immediate stop signaling**: Quit key triggers fast shutdown
- **No blocking UI**: Event loop never waits for provider

### Implementing Responsive Providers

When implementing `LogProvider`, ensure:

1. **Non-blocking `poll_logs()`**: Return immediately if no logs
2. **Interruptible waits**: Use `sleep_interruptible` for any delays
3. **Respect stop signal**: Check `should_stop` in long-running loops

Example of interruptible wait:

```rust
// ❌ Blocking - prevents fast quit
tokio::time::sleep(Duration::from_secs(1)).await;

// ✅ Interruptible - allows quit within 25ms
async fn sleep_interruptible(duration: Duration, should_stop: &Arc<Mutex<bool>>) {
    const CHECK_INTERVAL_MS: u64 = 25;
    let check_interval = Duration::from_millis(CHECK_INTERVAL_MS);

    let mut elapsed = Duration::ZERO;
    while elapsed < duration {
        if should_stop.load(Ordering::Relaxed) {
            return;
        }
        let sleep_time = check_interval.min(duration - elapsed);
        tokio::time::sleep(sleep_time).await;
        elapsed += sleep_time;
    }
}
```

### Configuration

Adjust polling intervals via `AppDesc`:

```rust
let mut desc = AppDesc::new(parser);

// Event polling (input responsiveness)
desc.event_poll_interval = Duration::from_millis(16); // ~60fps

// Provider polling (log ingestion frequency)
desc.poll_interval = Duration::from_millis(100); // 10Hz

start_with_desc(&mut terminal, provider, desc)?;
```

## Core Concepts

### LogProvider Trait

Provides raw log data from any source (files, network, APIs, etc.):

```rust
pub trait LogProvider: Send {
    /// initialize resources (called once at startup)
    fn start(&mut self) -> Result<()>;

    /// cleanup resources (called once at shutdown)
    fn stop(&mut self) -> Result<()>;

    /// poll for new logs (non-blocking, returns raw strings)
    fn poll_logs(&mut self) -> Result<Vec<String>>;
}
```

**Key points:**

- `poll_logs()` must be **non-blocking** - return empty vec if no logs available
- Returns raw **strings**, not parsed `LogItem`s
- Called repeatedly at configured interval (default: 100ms)

### LogParser Trait

Parses raw strings and formats them for display:

```rust
pub trait LogParser: Send + Sync {
    /// parse raw log string into structured LogItem (return None to filter)
    fn parse(&self, raw_log: &str) -> Option<LogItem>;

    /// format log for display at given detail level (0-4)
    fn format_preview(&self, item: &LogItem, detail_level: u8) -> String;

    /// extract searchable text for filtering
    fn get_searchable_text(&self, item: &LogItem, detail_level: u8) -> String;

    /// format for clipboard (optional, has default)
    fn make_yank_content(&self, item: &LogItem) -> String { /* default impl */ }

    /// max detail level supported (optional, default: 4)
    fn max_detail_level(&self) -> u8 { 4 }
}
```

### LogItem Structure

Structured representation of a log entry:

```rust
pub struct LogItem {
    pub id: Uuid,                       // auto-generated unique ID
    pub time: String,                   // timestamp (auto-generated or custom)
    pub content: String,                // parsed log message
    pub raw_content: String,            // original log line
    pub metadata: HashMap<String, String>, // extensible key-value storage
}
```

Use the builder pattern to add metadata:

```rust
let log = LogItem::new(
    "Application started".to_string(),
    "2025-01-15 10:30:00 INFO main.rs Application started".to_string(),
)
.with_metadata("level", "INFO")
.with_metadata("module", "main")
.with_metadata("severity", "1");
```

## Configuration

Customize behavior with `AppDesc`:

```rust
use std::time::Duration;
use lazylog_framework::{AppDesc, start_with_desc};
use std::sync::Arc;

let parser = Arc::new(MyParser);
let mut desc = AppDesc::new(parser);

desc.poll_interval = Duration::from_millis(50);  // poll every 50ms
desc.ring_buffer_size = 32768;                   // 32K log capacity
desc.show_debug_logs = true;                     // show debug panel

start_with_desc(&mut terminal, provider, desc)?;
```

## Keybindings

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` / `↑` / `↓` | Move to prev/next log |
| `d` | Jump to bottom (latest log) |
| `h` / `l` / `←` / `→` | Horizontal scrolling |
| `Space` | Make selected log visible in view |
| Mouse scroll | Vertical scrolling |
| `Shift` + Mouse scroll | Horizontal scrolling |

### Actions

| Key | Action |
|-----|--------|
| `/` | Enter filter mode |
| `y` | Copy current log to clipboard |
| `a` | Copy all displayed logs to clipboard |
| `c` | Clear all logs |
| `w` | Toggle text wrapping |
| `[` | Decrease detail level |
| `]` | Increase detail level |
| `Esc` | Go back / Clear filter |
| `q` | Quit program |
| `Ctrl+c` | Quit program |

### Focus

| Key | Action |
|-----|--------|
| `1` / `2` / `3` | Toggle focus on panel 1/2/3 |

### Help

| Key | Action |
|-----|--------|
| `?` | Show/hide help popup |
| `b` | Toggle debug logs visibility |

## Detail Levels

Control how much information is displayed (0-4):

Your parser defines what each level shows via `format_preview()`. Common convention:

| Level | Description |
|-------|-------------|
| 0 | Content only (minimal) |
| 1 | Time + content |
| 2 | Time + level + content |
| 3 | Time + level + module + content |
| 4 | All fields (maximum detail) |

Users can adjust levels with `+`/`-` keys to progressively reveal more information.

## Examples

See the `examples/` directory for complete working implementations:

### Simple Example

Generate dummy logs to demonstrate basic usage:

```bash
cargo run --example simple
```

### File Tailing Example

Tail a log file (like `tail -f`):

```bash
cargo run --example file -- /path/to/logfile.log

# or generate test logs:
while true; do echo "$(date) Test log"; sleep 1; done > /tmp/test.log
cargo run --example file -- /tmp/test.log
```

### Structured JSON Example

Parse JSON logs with metadata and detail levels:

```bash
cargo run --example structured
```

## Advanced Examples

### File-based Provider with Tailing

```rust
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};

struct FileProvider {
    reader: Option<BufReader<File>>,
    path: String,
}

impl LogProvider for FileProvider {
    fn start(&mut self) -> Result<()> {
        let file = File::open(&self.path)?;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::End(0))?; // start at end (tail mode)
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
            while reader.read_line(&mut line)? > 0 {
                if !line.trim().is_empty() {
                    logs.push(line.trim().to_string());
                }
                line.clear();
            }
        }
        Ok(logs)
    }
}
```

### JSON Parser with Metadata

```rust
struct JsonParser;

impl LogParser for JsonParser {
    fn parse(&self, raw_log: &str) -> Option<LogItem> {
        let json: serde_json::Value = serde_json::from_str(raw_log).ok()?;

        Some(LogItem::new(
            json["message"].as_str()?.to_string(),
            raw_log.to_string(),
        )
        .with_metadata("level", json["level"].as_str().unwrap_or("INFO"))
        .with_metadata("module", json["module"].as_str().unwrap_or("")))
    }

    fn format_preview(&self, item: &LogItem, level: u8) -> String {
        match level {
            0 => item.content.clone(),
            1 => format!("[{}] {}", item.time, item.content),
            2 => format!("[{}] [{}] {}",
                item.time,
                item.get_metadata("level").unwrap_or(""),
                item.content),
            _ => format!("[{}] [{}] [{}] {}",
                item.time,
                item.get_metadata("level").unwrap_or(""),
                item.get_metadata("module").unwrap_or(""),
                item.content),
        }
    }

    fn get_searchable_text(&self, item: &LogItem, level: u8) -> String {
        if level >= 2 {
            format!("{} {} {}",
                item.get_metadata("level").unwrap_or(""),
                item.get_metadata("module").unwrap_or(""),
                item.content)
        } else {
            item.content.clone()
        }
    }
}
```

### UDP Syslog Receiver

```rust
use std::net::UdpSocket;

struct SyslogProvider {
    socket: UdpSocket,
}

impl SyslogProvider {
    fn new(addr: &str) -> Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?; // critical for non-blocking poll
        Ok(Self { socket })
    }
}

impl LogProvider for SyslogProvider {
    fn start(&mut self) -> Result<()> { Ok(()) }
    fn stop(&mut self) -> Result<()> { Ok(()) }

    fn poll_logs(&mut self) -> Result<Vec<String>> {
        let mut logs = Vec::new();
        let mut buf = [0u8; 65536];

        loop {
            match self.socket.recv(&mut buf) {
                Ok(size) => {
                    let msg = String::from_utf8_lossy(&buf[..size]);
                    logs.push(msg.to_string());
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }

        Ok(logs)
    }
}
```

## Use Cases

- **Application logs** - Tail local log files
- **Container logs** - Monitor Docker/Kubernetes logs
- **Syslog viewer** - UDP/TCP syslog receiver
- **Database logs** - Stream from PostgreSQL, MongoDB, etc.
- **API logs** - Fetch from logging services (Elasticsearch, Loki, etc.)
- **SSH logs** - Remote log tailing via SSH
- **Multi-source aggregator** - Combine multiple log sources
- **Device logs** - Monitor mobile devices (iOS, Android)
- **Custom protocols** - Any streaming log source

## Performance

- **Memory-efficient**: Ring buffer prevents unbounded growth (default: 16K items)
- **Lock-free**: Uses ringbuf crate for zero-allocation producer/consumer
- **Lazy rendering**: Only visible logs are formatted and drawn
- **Parallel filtering**: Uses rayon for fast regex filtering on large log sets
- **Non-blocking**: Provider runs in background thread, UI stays responsive

## Requirements

- Rust 1.70+
- Terminal with ANSI color support
- For best experience: 256 colors, mouse support

## Testing

The framework includes doctests for all public APIs. Run them with:

```bash
cargo test --doc
```

## Documentation

Full API documentation is available at [docs.rs/lazylog-framework](https://docs.rs/lazylog-framework).

Generate local documentation:

```bash
cargo doc --open
```

## License

MIT OR Apache-2.0

## Contributing

Contributions welcome! Please:

1. Check existing issues or create one
2. Fork and create a feature branch
3. Add tests for new functionality
4. Ensure `cargo test` and `cargo clippy` pass
5. Submit a pull request

## Credits

Built with:

- [ratatui](https://github.com/ratatui-org/ratatui) - Terminal UI framework
- [crossterm](https://github.com/crossterm-rs/crossterm) - Terminal manipulation
- [ringbuf](https://github.com/agerasev/ringbuf) - Lock-free ring buffer
- [regex](https://github.com/rust-lang/regex) - Regular expressions
- [rayon](https://github.com/rayon-rs/rayon) - Parallel filtering

## Related Projects

- [lazylog](https://github.com/your-org/lazylog) - Reference implementation with file/iOS/Android providers
