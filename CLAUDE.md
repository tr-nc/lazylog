# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Rules

if the code is already self-explainable, remove the comments.

for inline comments, don't capitalize the first letter.
  
## Project Overview

dhlog is a terminal-based log file viewer built with Rust and ratatui. It provides real-time log monitoring with vim-like navigation, structured log parsing, and efficient handling of large files through memory-mapped access.

## Development Commands

### Build and Run

```bash
# Build the project
cargo build

# Build release version
cargo build --release

# Run with automatic log file selection
cargo run -- -a

# Run on a specific log file
cargo run -- path/to/logfile.log

# Run with debug logging
cargo run -- --verbose
```

### Code Quality

```bash
# Format code
cargo fmt

# Check compilation without building
cargo check
```

### Testing

Currently no test suite exists. Use `cargo check` to verify compilation.

## Architecture

### Core Components

**Event-Driven Architecture**: The application follows a poll-based event loop pattern:

- `src/app.rs`: Main application state, event handling, and UI coordination
- `src/log_list.rs`: Core navigation logic with circular (j/k) and traditional (arrow/mouse) modes
- `src/log_parser.rs`: Regex-based structured log parsing with level detection
- `src/main.rs`: CLI argument parsing and application initialization

**File Processing Pipeline**:

1. Memory-mapped file access via `memmap2` for performance
2. Delta processing - only new content is parsed on updates
3. Regex-based log structure extraction (timestamp, level, tag, message)
4. Bounded buffer management to prevent memory exhaustion

**UI System**:

- Modular rendering: header, list, details, footer components
- Dual navigation modes: circular (vim j/k) vs traditional (arrow keys/mouse)
- Real-time scrollbar synchronization with list state
- Color-coded log levels with theme support

### Key Design Patterns

**State Management**: Centralized in `App` struct with separate filtered/unfiltered log lists
**Event Handling**: Pattern matching on crossterm events with separate keyboard/mouse handlers
**Error Handling**: Uses `anyhow` and `color-eyre` for comprehensive error context
**File Monitoring**: Poll-based with 100ms intervals, detects rotation via metadata changes

### Navigation Implementation

The application implements two distinct navigation behaviors:

- **Circular Navigation** (j/k keys): Wraps around at list boundaries
- **Traditional Navigation** (arrow keys/mouse): Stops at first/last items

This is controlled in `src/app.rs` where different key events call different navigation methods on the `LogList` struct.

## Extension Points

**Adding New Key Bindings**: Modify `handle_key()` in `src/app.rs`
**Custom Log Parsing**: Extend regex patterns in `src/log_parser.rs`
**New UI Components**: Implement ratatui's `Widget` trait and integrate in `src/app.rs` render method
**File Discovery**: Modify log file selection logic in main.rs argument handling
