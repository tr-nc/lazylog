# lazylog-framework

A powerful, extensible framework for building terminal-based log viewers with vim-like navigation and real-time monitoring capabilities.

## Features

- 🚀 **Provider-based architecture** - Pluggable log sources
- ⌨️ **Vim-like navigation** - j/k, gg/G, Ctrl+d/u, and more
- 🔄 **Real-time streaming** - Monitor logs as they arrive
- 🔍 **Filtering** - Dynamic log filtering with `/` search
- 📊 **Detail levels** - Control information density
- 🖱️ **Mouse support** - Click and scroll
- 🎨 **Tailwind colors** - Beautiful, modern UI
- 📋 **Clipboard integration** - Yank logs with `y`

## Installation

```toml
[dependencies]
lazylog-framework = "0.1"
```

## Quick Start

```rust
use lazylog_framework::{LogProvider, LogItem, start_with_provider};
use anyhow::Result;

// 1. Implement LogProvider for your log source
struct MyLogProvider {
    // Your state here
}

impl LogProvider for MyLogProvider {
    fn start(&mut self) -> Result<()> {
        // Setup resources (open files, connect to streams, etc.)
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        // Cleanup resources
        Ok(())
    }

    fn poll_logs(&mut self) -> Result<Vec<LogItem>> {
        // Return new logs since last poll
        Ok(vec![
            LogItem::new(
                "2025-01-15 10:30:00".to_string(),  // time
                "INFO".to_string(),                 // level
                "MyApp".to_string(),                // origin
                "startup".to_string(),              // tag
                "Application started".to_string(),  // content
                "Raw log line here".to_string(),    // raw_content
            )
        ])
    }
}

// 2. Run the application
fn main() -> Result<()> {
    let mut terminal = /* setup ratatui terminal */;
    let provider = MyLogProvider { /* ... */ };
    lazylog_framework::start_with_provider(&mut terminal, provider)?;
    Ok(())
}
```

## Architecture

```
┌─────────────────────────────────────┐
│   Your LogProvider Implementation  │
│  (file, socket, API, database, etc) │
└─────────────────┬───────────────────┘
                  │
                  ↓
         ┌────────────────────┐
         │   Ring Buffer      │
         │   (16K capacity)   │
         └────────┬───────────┘
                  │
                  ↓
      ┌──────────────────────┐
      │  lazylog-framework   │
      │  - Rendering         │
      │  - Navigation        │
      │  - Filtering         │
      │  - UI management     │
      └──────────┬───────────┘
                 │
                 ↓
           ┌─────────────┐
           │  Terminal   │
           └─────────────┘
```

## LogProvider Trait

The core abstraction for log sources:

```rust
pub trait LogProvider: Send {
    /// Initialize resources (called once at startup)
    fn start(&mut self) -> Result<()>;

    /// Cleanup resources (called once at shutdown)
    fn stop(&mut self) -> Result<()>;

    /// Poll for new logs (called repeatedly, non-blocking)
    fn poll_logs(&mut self) -> Result<Vec<LogItem>>;
}
```

### Poll Interval

By default, `poll_logs()` is called every 100ms. Customize with `AppDesc`:

```rust
use std::time::Duration;
use lazylog_framework::{AppDesc, start_with_desc};

let desc = AppDesc {
    poll_interval: Duration::from_millis(50), // Poll every 50ms
    show_debug_logs: true,
    ring_buffer_size: 32768, // 32K buffer
};

start_with_desc(&mut terminal, provider, desc)?;
```

## LogItem Structure

```rust
pub struct LogItem {
    pub id: Uuid,              // Auto-generated unique ID
    pub time: String,          // Timestamp (free-form)
    pub level: String,         // Log level (INFO, ERROR, etc.)
    pub origin: String,        // Source/component
    pub tag: String,           // Category/tag
    pub content: String,       // Parsed content
    pub raw_content: String,   // Original log line
}
```

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `g` `g` | Go to top |
| `G` | Go to bottom |
| `Ctrl+d` | Page down |
| `Ctrl+u` | Page up |
| `/` | Enter filter mode |
| `Esc` | Clear filter / Exit filter mode |
| `a` | Toggle autoscroll |
| `+` | Increase detail level |
| `-` | Decrease detail level |
| `w` | Toggle text wrapping |
| `y` | Yank (copy) selected log |
| `d` | Toggle debug logs |
| `?` | Show help |
| `1` / `2` / `3` | Focus block 1/2/3 |
| `q` | Quit |

## Detail Levels

Control how much information is displayed:

| Level | Display |
|-------|---------|
| 0 - ContentOnly | `content` |
| 1 - Basic | `[time] content` |
| 2 - Medium | `[time] [tag] content` |
| 3 - Detailed | `[time] [tag] [origin] content` |
| 4 - Full | `[time] [tag] [origin] [level] content` |

## Example Providers

### File-based Provider

```rust
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};

struct FileLogProvider {
    file: BufReader<File>,
    position: u64,
}

impl FileLogProvider {
    fn new(path: &str) -> Result<Self> {
        let file = BufReader::new(File::open(path)?);
        Ok(Self { file, position: 0 })
    }
}

impl LogProvider for FileLogProvider {
    fn start(&mut self) -> Result<()> { Ok(()) }
    fn stop(&mut self) -> Result<()> { Ok(()) }

    fn poll_logs(&mut self) -> Result<Vec<LogItem>> {
        let mut logs = Vec::new();
        let mut line = String::new();

        while self.file.read_line(&mut line)? > 0 {
            if !line.trim().is_empty() {
                logs.push(LogItem::new(
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    line.trim().to_string(),
                    line.clone(),
                ));
            }
            line.clear();
        }

        Ok(logs)
    }
}
```

### Socket-based Provider

```rust
use std::net::UdpSocket;

struct SyslogProvider {
    socket: UdpSocket,
}

impl SyslogProvider {
    fn new(addr: &str) -> Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket })
    }
}

impl LogProvider for SyslogProvider {
    fn start(&mut self) -> Result<()> { Ok(()) }
    fn stop(&mut self) -> Result<()> { Ok(()) }

    fn poll_logs(&mut self) -> Result<Vec<LogItem>> {
        let mut logs = Vec::new();
        let mut buf = [0u8; 65536];

        loop {
            match self.socket.recv(&mut buf) {
                Ok(size) => {
                    let msg = String::from_utf8_lossy(&buf[..size]);
                    logs.push(parse_syslog(&msg));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }

        Ok(logs)
    }
}
```

## Customization

### AppDesc Configuration

```rust
use std::time::Duration;

let config = AppDesc {
    poll_interval: Duration::from_millis(100),  // How often to poll provider
    show_debug_logs: false,                     // Show [3] Debug Logs panel
    ring_buffer_size: 16384,                    // Max logs in memory
};
```

### Log Parsing

Extend `LogItem` with custom parsing in your provider:

```rust
fn parse_json_log(line: &str) -> LogItem {
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    LogItem::new(
        parsed["timestamp"].as_str().unwrap_or("").to_string(),
        parsed["level"].as_str().unwrap_or("INFO").to_string(),
        parsed["service"].as_str().unwrap_or("").to_string(),
        parsed["module"].as_str().unwrap_or("").to_string(),
        parsed["message"].as_str().unwrap_or("").to_string(),
        line.to_string(),
    )
}
```

## Use Cases

- **Application logs** - Tail local log files
- **Container logs** - Monitor Docker/Kubernetes logs
- **Syslog viewer** - UDP/TCP syslog receiver
- **Database logs** - Stream from PostgreSQL, MongoDB, etc.
- **API logs** - Fetch from logging services
- **SSH logs** - Remote log tailing
- **Multi-source aggregator** - Combine multiple log sources

## Performance

- **Memory-efficient**: Ring buffer prevents unbounded growth
- **Non-blocking**: Provider runs in background thread
- **Lazy rendering**: Only visible logs are rendered
- **Fast filtering**: Efficient text matching

## Requirements

- Rust 1.70+
- Terminal with ANSI color support
- For best experience: 256 colors, mouse support

## License

MIT OR Apache-2.0

## Credits

Built with:
- [ratatui](https://github.com/ratatui-org/ratatui) - TUI framework
- [crossterm](https://github.com/crossterm-rs/crossterm) - Terminal manipulation
- [ringbuf](https://github.com/agerasev/ringbuf) - Lock-free ring buffer

## Contributing

Contributions welcome! Please open an issue or PR.
