# Lazylog

A fast, minimal, terminal-based log file viewer with vim-like navigation and real-time monitoring.

Lazylog provides instant log file access with structured parsing, smooth scrolling, and efficient handling of large files through memory-mapped I/O.

## Features

- **Multiple log sources** - Support for DYEH file logs, iOS device logs, and Android device logs
- **Headless streaming** - Dump parsed logs to stdout for scripting and coding agent workflows
- **Real-time monitoring** - Automatically follows log updates like `tail -f`
- **Vim-like navigation** - Use `j/k` or arrow keys for navigation, `h/l` for horizontal scrolling
- **Smart log parsing** - Automatically detects timestamps, log levels, tags, and messages
- **Color-coded severity** - Visual distinction between DEBUG, INFO, WARN, ERROR, and FATAL levels
- **Memory efficient** - Handles large files with memory-mapped access and ring buffers (16K capacity)
- **Cross-platform** - Runs on Linux, macOS, and Windows
- **Framework library** - `lazylog-framework` available as a reusable crate for custom log viewers
- **Responsive UI** - Interruptible sleeps ensure snappy key responses even when no device is connected

## Performance

Lazylog is designed for responsiveness:

- **Event polling** at 16ms interval (~60fps) ensures immediate key and mouse response
- **Provider polling** at 100ms interval for efficient log ingestion
- **Interruptible sleeps** check stop signals every 25ms, allowing quit to interrupt long waits
- **Immediate stop signaling** on quit keys (`q` or `Ctrl+C`) triggers fast shutdown

This means pressing `q` exits within ~25-50ms even when no device is connected (was up to 1s before).

## Installation

### Homebrew

```bash
# Install
brew install tr-nc/tap/lazylog

# Upgrade
brew upgrade lazylog

# Uninstall
brew uninstall lazylog
```

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
# Run lazylog with DYEH preview logs
cargo run -- --dyeh-preview

# Use DYEH editor logs
cargo run -- --dyeh-editor

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

# Stream logs to stdout without the TUI
cargo run -- --headless --dyeh-preview
```

### Headless mode

Use `--headless` to skip the TUI and stream logs directly to stdout.

```bash
# Headless mode works with filters and all existing providers
cargo run -- --headless --android-effect --filter "ERROR"

# Stream iOS logs non-interactively
cargo run -- --headless --ios
```

Headless mode behavior:

- Streams forever for live providers until interrupted
- Reuses the existing provider and parser for the selected mode
- Applies startup filters from `--filter`
- Prints each matching parsed item using its full `raw_content`

### Key bindings

| Key                  | Action                                             |
| -------------------- | -------------------------------------------------- |
| `j`/`k` or `↑`/`↓`   | Navigate up/down through log items                 |
| `d`                  | Jump to bottom (newest) log item                   |
| `h`/`l` or `←`/`→`   | Horizontal scrolling (left/right)                  |
| `Space`              | Make selected log visible in view                  |
| `[`/`]`              | Decrease/increase detail level (0-4)               |
| `/` or `f`           | Enter filter mode                                  |
| `v`                  | Enter visual mode                                  |
| `y`                  | Yank (copy) selected log item(s) to clipboard      |
| `a`                  | Yank (copy) all displayed logs to clipboard        |
| `c`                  | Clear all logs                                     |
| `w`                  | Toggle text wrapping                               |
| `m`                  | Toggle mouse capture (disable to select/copy text) |
| `b`                  | Toggle debug logs visibility                       |
| `1`/`2`/`3`          | Focus logs/details/debug panel                     |
| `?`                  | Show/hide help popup                               |
| `Esc`                | Exit visual mode / Go back / Clear filter          |
| `q`                  | Quit                                               |
| `Ctrl+C`             | Quit                                               |
| Mouse scroll         | Vertical scrolling through logs or focused panel   |
| Shift + Mouse scroll | Horizontal scrolling                               |
| Mouse click          | Focus panel, select item, or drag scrollbar        |

### Filter mode

- Type to filter logs by content
- `Enter` - Apply filter and exit filter mode
- `Esc` - Cancel filter and exit filter mode

### Visual mode

- Press `v` on a log item to start visual selection
- Press `j`/`k` to expand or shrink the consecutive selection
- Press `y` to copy selected log items to the clipboard
- Press `Esc` to exit visual mode
- Filter mode cannot be entered while visual mode is active
- When multiple log items are selected, the details panel shows a hint instead of item details

### Navigation

- **Logs panel**: Navigate through log items, newest at top (focus with `1`)
- **Details panel**: Shows expanded details for selected log item (focus with `2`)
- **Debug panel**: Shows application debug messages (focus with `3`, toggle with `b`)
- Use mouse click or `1`/`2`/`3` to focus different panels and scroll within them

## Development

### Prerequisites

- Rust toolchain 1.77+ ([install via rustup](https://rustup.rs))
- **iOS support** (optional): Requires `idevicesyslog` - install via `brew install libimobiledevice` on macOS
- **Android support** (optional): Requires `adb` - install via `brew install android-platform-tools` on macOS

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
