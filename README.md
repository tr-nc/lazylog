# Lazylog

A fast, minimal, terminal-based log file viewer with vim-like navigation and real-time monitoring.

Lazylog provides instant log file access with structured parsing, smooth scrolling, and efficient handling of large files through memory-mapped I/O.

## Features

- **Multiple log sources** - Support for DYEH file logs, iOS device logs, and Android device logs
- **Real-time monitoring** - Automatically follows log updates like `tail -f`
- **Vim-like navigation** - Use `j/k` or arrow keys for navigation, `h/l` for horizontal scrolling
- **Smart log parsing** - Automatically detects timestamps, log levels, tags, and messages
- **Color-coded severity** - Visual distinction between DEBUG, INFO, WARN, ERROR, and FATAL levels
- **Memory efficient** - Handles large files with memory-mapped access and ring buffers (16K capacity)
- **Cross-platform** - Runs on Linux, macOS, and Windows
- **Framework library** - `lazylog-framework` available as a reusable crate for custom log viewers

## Installation

### Prerequisites

- Rust toolchain 1.77+ ([install via rustup](https://rustup.rs))
- **iOS support** (optional): Requires `idevicesyslog` - install via `brew install libimobiledevice` on macOS
- **Android support** (optional): Requires `adb` - install via `brew install android-platform-tools` on macOS

### Build from source

```bash
git clone https://github.com/tr-nc/lazylog.git
cd lazylog
cargo build --release

# Run the built binary
./target/release/lazylog --help
```

## Usage

### Basic usage

```bash
# Run lazylog with DYEH log provider (default)
cargo run

# Use iOS log provider
cargo run -- --ios

# Use iOS log provider (effect mode)
cargo run -- --ios-effect

# Use Android log provider
cargo run -- --android

# Use Android log provider (effect mode)
cargo run -- --android-effect

# Apply filter on startup
cargo run -- --filter "ERROR"
```

### Key bindings

| Key                  | Action                                             |
| -------------------- | -------------------------------------------------- |
| `j`/`k` or `↑`/`↓`   | Navigate up/down through log items                 |
| `d`                  | Jump to bottom (newest) log item                   |
| `h`/`l` or `←`/`→`   | Horizontal scrolling (left/right)                  |
| `Space`              | Make selected log visible in view                  |
| `[`/`]`              | Decrease/increase detail level (0-4)               |
| `/`                  | Enter filter mode                                  |
| `y`                  | Yank (copy) current log item to clipboard          |
| `a`                  | Yank (copy) all displayed logs to clipboard        |
| `c`                  | Clear all logs                                     |
| `w`                  | Toggle text wrapping                               |
| `m`                  | Toggle mouse capture (disable to select/copy text) |
| `b`                  | Toggle debug logs visibility                       |
| `1`/`2`/`3`          | Focus logs/details/debug panel                     |
| `?`                  | Show/hide help popup                               |
| `Esc`                | Go back / Clear filter                             |
| `q`                  | Quit                                               |
| `Ctrl+C`             | Quit                                               |
| Mouse scroll         | Vertical scrolling through logs or focused panel   |
| Shift + Mouse scroll | Horizontal scrolling                               |
| Mouse click          | Focus panel, select item, or drag scrollbar        |

### Filter mode

- Type to filter logs by content
- `Enter` - Apply filter and exit filter mode
- `Esc` - Cancel filter and exit filter mode

### Navigation

- **Logs panel**: Navigate through log items, newest at top (focus with `1`)
- **Details panel**: Shows expanded details for selected log item (focus with `2`)
- **Debug panel**: Shows application debug messages (focus with `3`, toggle with `b`)
- Use mouse click or `1`/`2`/`3` to focus different panels and scroll within them

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

# Run tests
cargo test

# Run linter
cargo clippy
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
