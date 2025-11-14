# Lazylog

A fast, minimal, terminal-based log file viewer with vim-like navigation and real-time monitoring.

Lazylog provides instant log file access with structured parsing, smooth scrolling, and efficient handling of large files through memory-mapped I/O.

## Features

- **Real-time monitoring** - Automatically follows log updates like `tail -f`
- **Vim-like navigation** - Use `j/k` for circular navigation or arrow keys for traditional movement
- **Smart log parsing** - Automatically detects timestamps, log levels, tags, and messages
- **Color-coded severity** - Visual distinction between DEBUG, INFO, WARN, ERROR, and FATAL levels
- **Memory efficient** - Handles large files with memory-mapped access and bounded buffers
- **Cross-platform** - Runs on Linux, macOS, and Windows

## Installation

### Prerequisites

- Rust toolchain 1.77+ ([install via rustup](https://rustup.rs))
- **VS Code terminal** - This program is developed and tested specifically for VS Code's integrated terminal. It may be buggy when run in macOS Terminal or other terminal emulators.

### Build from source

```bash
git clone https://github.com/tr-nc/lazylog.git
cd lazylog
cargo build --release
```

## Usage

### Basic usage

```bash
# Run lazylog (automatically monitors ~/Library/Application Support/DouyinAR/Logs/previewLog)
cargo run
```

### Key bindings

| Key | Action |
|-----|--------|
| `j`/`k` or `↑`/`↓` | Navigate up/down through log items |
| `g` | Go to first (newest) log item |
| `G` | Go to last (oldest) log item |
| `[`/`]` | Decrease/increase detail level (0-4) |
| `/` | Enter filter mode |
| `y` | Yank (copy) current log item to clipboard |
| `a` | Yank (copy) all displayed logs to clipboard |
| `c` | Clear all logs |
| `f` | Fold logs (not implemented) |
| `q` or `Esc` | Quit |
| `Ctrl+C` | Force quit |
| Mouse scroll | Scroll through logs or focused panel |
| Mouse click | Focus panel and select item |

### Filter mode

- Type to filter logs by content
- `Enter` - Apply filter
- `Esc` - Cancel filter and show all logs

### Navigation

- **Logs panel**: Navigate through log items, newest at top
- **Details panel**: Shows expanded details for selected log item
- **Debug panel**: Shows application debug messages
- Use mouse to focus different panels and scroll within them

## Development

### Build and test

```bash
# Check compilation
cargo check

# Format code
cargo fmt

# Build debug version
cargo build

# Build release version
cargo build --release
```

## Publish framework

```sh
cargo publish -p lazylog-framework
```

## Contributing

We welcome contributions, testing, and feedback! If you:

- **Find bugs** - Please report them with terminal environment details
- **Test on different terminals** - Help us improve compatibility beyond VS Code
- **Have feature ideas** - Share your suggestions for improvements
- **Want to contribute code** - Pull requests are welcome

Feel free to open issues or submit pull requests to help make lazylog better for everyone.

## Acknowledgments

Inspired by [lazygit](https://github.com/jesseduffield/lazygit) for the name and terminal UI design philosophy.

Built with [ratatui](https://github.com/ratatui-org/ratatui) for terminal UI and [crossterm](https://github.com/crossterm-rs/crossterm) for cross-platform terminal handling.

Co-authored and documented with [Claude Code](https://claude.ai/code).
