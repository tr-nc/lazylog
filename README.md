# Lazylog

A fast, minimal, terminal-based log file viewer with vim-like navigation and real-time monitoring.

Lazylog provides instant log file access with structured parsing, smooth scrolling, and efficient handling of large files through memory-mapped I/O.

## Features

- **Multiple log sources** - Support for DYEH file logs, iOS device logs, and Android device logs
- **Headless streaming** - Dump parsed logs to stdout for scripting workflows
- **Agent captures** - Save complete plain-text sessions to unique temporary files with no stdout log output
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
# Open the interactive Provider picker
cargo run --

# Run lazylog with DYEH preview logs
cargo run -- --dyeh-preview

# Use DYEH editor logs
cargo run -- --dyeh-editor

# Skip Provider selection and choose an iOS device, then an App
cargo run -- --ios

# Select the device interactively but skip the iOS App picker
cargo run -- --ios --ios-app effectcam
cargo run -- --ios --ios-app douyin

# Keep devicectl device/App control, but read logs with idevicesyslog
cargo run -- --ios --ios-app effectcam --ios-log-backend idevicesyslog

# Skip Provider selection and choose an Android device
cargo run -- --android

# Apply filter on startup
cargo run -- --filter "ERROR"

# Stream logs to stdout without the TUI
cargo run -- --headless --dyeh-preview
```

### Headless mode

Use `--headless` to skip the TUI and stream logs directly to stdout.

```bash
# Headless mode works with filters and all existing providers
cargo run -- --headless --android --filter "ERROR"

# Stream iOS logs non-interactively (an explicit app is required)
cargo run -- --headless --ios --ios-app effectcam

# Target an exact device ID from `lazylog agent inspect --json`
cargo run -- --headless --ios --ios-device 00008120-EXAMPLE \
  --ios-app effectcam
cargo run -- --headless --android --android-device emulator-5554

# Compare against the device-wide idevicesyslog stream
cargo run -- --headless --ios --ios-app effectcam --ios-log-backend idevicesyslog
```

Headless mode behavior:

- Streams forever for live providers until interrupted
- Reuses the existing provider and parser for the selected mode
- Applies startup filters from `--filter`
- Prints each matching parsed item using its full `raw_content`
- Requires `--ios-app effectcam|douyin` in iOS mode
- Uses `devicectl` for the iOS log stream by default; `--ios-log-backend idevicesyslog`
  switches only log capture, not device/App control

### Agent mode

Use `--agent` for coding-agent sessions. It writes every parsed item to a unique plain-text
temporary file and never sends log content to stdout.

```bash
# Inspect devices, App states, backends, and valid next commands without changing runtime state
cargo run -- agent inspect --json

# Capture for 30 seconds and use an automatically generated capture path
cargo run -- --agent --dyeh-preview --duration 30

# Capture iOS logs on an exact inspected device
cargo run -- --agent --ios --ios-device 00008120-EXAMPLE \
  --ios-app effectcam --duration 30

# Capture Android logs on an exact inspected device
cargo run -- --agent --android --android-device emulator-5554 --duration 30

# A/B test the legacy device syslog stream without changing the control path
cargo run -- --agent --ios --ios-app effectcam \
  --ios-log-backend idevicesyslog --duration 30
```

Agent mode behavior:

- `agent inspect --json` writes schema-versioned discovery data to stdout and nothing to stderr on
  success; it does not launch or terminate an App and does not start a capture
- Inspection reports explicit iOS and Android device counts; for every known target App it
  distinguishes installation and process presence, and on iOS also reports control readiness
- Every generated mobile capture command includes `--ios-device` or `--android-device`, so an agent
  can act on any listed device instead of silently falling back to the first one
- Process presence does not mean foreground state; cross-process Lazylog capture-session detection
  is explicitly reported as unsupported
- Inspection probes whether the optional `idevicesyslog` executable is available but never requires
  or installs it
- Creates a new capture file for every invocation under the OS temporary directory at
  `lazylog/agent-captures`
- Prints only the capture path, status changes, and final statistics to stderr; stdout stays empty
- Writes the complete parsed session without ANSI color codes
- Captures every parsed item; search the resulting file with tools such as `rg` after or during capture
- Stops and flushes cleanly on `Ctrl+C`, `SIGTERM`, or after `--duration`
- Requires `--ios-app effectcam|douyin` in iOS mode; Agent mode never opens a picker
- Checks for `idevicesyslog` only when that backend is explicitly selected

### 0.11 mobile CLI migration

Version 0.11 consolidates `--ios` and `--android` on the structured effect-log parsers. The former
`--ios-effect`, `--android-effect`, `-i`, `-ie`, `-a`, `-ae`, `-dyp`, and `-dye` spellings remain
accepted for this release and print a deprecation warning; use the canonical long options in new
commands. The removed unstructured full-device modes are not restored by those aliases.

`--ios-device <ID>` and `--android-device <SERIAL>` are available in Agent and headless modes.
Interactive mode continues to use the live Device picker.

### Connection status

Interactive, headless, and Agent modes use the same connection labels. iOS distinguishes an
unplugged USB cable, a disconnected device, an exited target App, and a failed capture connection.
Android reports device disconnects and capture failures; because its current source is global
`adb logcat`, it cannot reliably infer that one particular App exited.

Some Android builds publish each structured effect record twice: first under its original logcat
tag (which may be empty), then through the `Effect` mirror tag. Lazylog folds only a structurally
matched mirror pair: the PID, TID, level, complete payload, and immediate logcat timing must agree.
Single-channel builds, unmatched `Effect` records, and repeated source writes are preserved. This
keeps release builds such as Douyin working while removing the dual-channel copies seen in internal
builds and EffectCam, without general content-based deduplication.

Interactive sessions use one progressive picker. Its Provider tab lists iOS, Android, DYEH
preview, and DYEH editor. iOS then exposes Device and App tabs; Android exposes Device; DYEH needs
no additional selection. Switching an earlier selection invalidates dependent later selections.
All picker lists can be controlled with arrow keys or the mouse, and the picker refreshes device
and iOS App availability hints in the background. Once a mobile Provider is committed, its device
list is refreshed about once per second, so plugging, unplugging, and replugging devices is reflected
without reopening the picker.

While the Provider tab is active, moving the highlight previews that Provider's downstream tabs:
iOS shows Device and App, Android shows Device, and DYEH shows neither. Previewing is display-only;
only Enter or clicking a Provider row commits the Provider and invalidates dependent selections,
so highlighting Android does not stop or replace a previously selected iOS session.

If an iOS target App exits, Lazylog returns to the App tab while retaining the selected device. If
the selected iOS or Android device disconnects, Lazylog returns to the Device tab. From a log view,
press `Esc` twice within 500ms to return to the picker; press `q` anywhere to exit Lazylog. The iOS
App list reports installed, not installed, or detection failed as a hint. A preset App remains
selectable in all three states, and confirming it asks `devicectl` to terminate any existing
process and relaunch the App with its console attached.

`devicectl` remains the iOS control plane for device discovery, App inspection, launch, and target
exit monitoring. The default log backend attaches `devicectl` app-console. Passing
`--ios-log-backend idevicesyslog` launches the same selected App through `devicectl`, then reads the
selected device's syslog with `idevicesyslog -u <UDID>`. This optional backend is useful for A/B
testing log coverage; because the stream is device-wide, the effect parser still retains only
structured effect logs. Capture commands never require or ask users to install `idevicesyslog`
unless this backend is selected for that invocation; read-only Agent inspection only reports whether
the executable is already available. Before either backend changes the target App,
Lazylog makes one read-only control request with a five-second limit. A device that is discoverable
but not controllable produces an actionable error instead of entering a partially connected session.
The default `devicectl` backend gives the process a Lazylog-owned pseudo-terminal while keeping
devicectl stderr separate. App logs are consumed only from stdout; stderr is drained as a bounded
diagnostics channel. This prevents cross-channel console mirrors from entering the log stream
without comparing or deleting records by content, so two identical records written to stdout are
still preserved. The optional `idevicesyslog` backend preserves its raw stream for coverage
comparisons.

The generated Agent capture path is stored under the OS temporary directory in
`lazylog/agent-captures`. A complete capture means everything Lazylog observed during that
invocation; live providers do not necessarily include logs from before startup.

### Key bindings

| Key                  | Action                                             |
| -------------------- | -------------------------------------------------- |
| `j`/`k` or `↑`/`↓`   | Navigate up/down through log items                 |
| `d`                  | Jump to bottom (newest) log item                   |
| `h`/`l` or `←`/`→`   | Horizontal scrolling (left/right)                  |
| `Space`              | Make selected log visible in view                  |
| `[`/`]`              | Decrease/increase detail level (0-4)               |
| `/` or `f`           | Enter filter mode                                  |
| `v`                  | Toggle visual mode                                 |
| `y`                  | Yank (copy) selected log item(s) to clipboard      |
| `a`                  | Yank (copy) all displayed logs to clipboard        |
| `c`                  | Clear all logs                                     |
| `w`                  | Toggle text wrapping                               |
| `m`                  | Toggle mouse capture (disable to select/copy text) |
| `b`                  | Toggle debug logs visibility                       |
| `1`/`2`/`3`          | Focus logs/details/debug panel                     |
| `?`                  | Show/hide help popup                               |
| `Esc`                | Exit visual mode / Clear filter                     |
| `Esc` twice in 500ms | Return from a log view to the picker                |
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

- Press `v` on a log item to start visual selection; press it again to exit
- Press `j`/`k` to expand or shrink the consecutive selection
- Press `y` to copy selected log items to the clipboard
- Press `v` or `Esc` to exit visual mode
- Filter mode cannot be entered while visual mode is active
- When multiple log items are selected, the details panel shows a hint instead of item details

### Navigation

- **Logs panel**: Navigate through log items, newest at top (focus with `1`)
- **Details panel**: Shows expanded details for selected log item (focus with `2`)
- **Debug panel**: Shows application debug messages (focus with `3`, toggle with `b`)
- Use mouse click or `1`/`2`/`3` to focus different panels and scroll within them

## Development

### Prerequisites

- Rust toolchain 1.88+ ([install via rustup](https://rustup.rs)); CI checks this minimum explicitly
- **iOS support** (optional): Requires a current Xcode with `devicectl`
- **Legacy iOS log backend** (optional): `brew install libimobiledevice`; required only for an
  invocation that explicitly passes `--ios-log-backend idevicesyslog`
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
