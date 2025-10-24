# Lazylog Workspace Structure

## Overview

Lazylog is now organized as a Cargo workspace with three separate crates:

1. **`lazylog-framework`** - Open-source terminal log viewer framework
2. **`lazylog-dyeh`** - Internal DYEH log provider (ByteDance-specific)
3. **`lazylog`** - Internal binary that combines framework + DYEH provider

## Directory Structure

```
lazylog/
├── Cargo.toml                        # Workspace root
├── Cargo.lock                        # Lockfile for all workspace members
│
├── lazylog-framework/                # 📦 OPEN SOURCE
│   ├── Cargo.toml                   # Framework dependencies only
│   ├── README.md                     # (TODO) Framework documentation
│   ├── LICENSE-MIT                   # (TODO)
│   ├── LICENSE-APACHE                # (TODO)
│   └── src/
│       ├── lib.rs                    # Public API
│       ├── provider/                 # Provider trait & types
│       │   ├── mod.rs
│       │   └── log_item.rs
│       ├── app.rs                    # Main application logic
│       ├── app_block.rs              # UI block abstraction
│       ├── content_line_maker.rs     # Text rendering
│       ├── log_list.rs               # List state management
│       ├── log_parser.rs             # LogItem helpers
│       ├── status_bar.rs             # Status bar
│       ├── theme.rs                  # Colors & styling
│       └── ui_logger.rs              # Debug logging
│
├── lazylog-dyeh/                     # 🔒 INTERNAL
│   ├── Cargo.toml                   # publish = false
│   └── src/
│       ├── lib.rs                    # Exports DyehLogProvider
│       ├── provider.rs               # DYEH log provider impl
│       ├── parser.rs                 # DYEH log format parsing
│       ├── file_finder.rs            # previewLog discovery
│       └── metadata.rs               # File change detection (macOS)
│
└── lazylog-bin/                      # 🔒 INTERNAL
    ├── Cargo.toml                   # publish = false
    └── src/
        └── main.rs                   # Binary entrypoint
```

## Crate Details

### 1. lazylog-framework (Open Source)

**Purpose:** Generic TUI framework for building log viewers

**Public API:**
```rust
// Core types
pub use provider::{LogItem, LogDetailLevel, LogProvider, spawn_provider_thread};

// Application runner
pub use app::{AppDesc, start_with_provider, start_with_desc};
```

**Features:**
- Provider-based architecture
- Vim-like navigation (j/k, gg/G, Ctrl+d/u)
- Real-time log streaming
- Filtering and search
- Detail level control
- Mouse support
- Customizable via `AppDesc`

**Dependencies:**
- `ratatui` - TUI framework
- `crossterm` - Terminal control
- `ringbuf` - Lock-free ring buffer
- `arboard` - Clipboard support
- No platform-specific code
- No DYEH-specific code

**Can be published to:** crates.io ✅

### 2. lazylog-dyeh (Internal)

**Purpose:** DYEH log provider implementation

**Exports:**
```rust
pub use provider::DyehLogProvider;
```

**DYEH-Specific Features:**
- DouyinAR path resolution (`~/Library/Application Support/DouyinAR`)
- Scans both `Logs/` and `Log/` subdirectories
- Finds `previewLog` directories recursively
- Parses DYEH format: `## YYYY-MM-DD HH:MM:SS`
- Header parsing: `[origin] LEVEL ## [TAG] content`
- Special event detection (PAUSE/RESUME)
- Memory-mapped file I/O
- Log rotation handling

**Dependencies:**
- `lazylog-framework` (path dependency)
- `memmap2` - Memory-mapped files
- `dirs` - Home directory lookup
- `libc` - File metadata (macOS)

**Publishing:** `publish = false` 🔒

### 3. lazylog (Internal Binary)

**Purpose:** ByteDance internal log viewer for DYEH

**What it does:**
```rust
fn main() {
    let log_dir = dirs::home_dir()
        .join("Library/Application Support/DouyinAR");
    let provider = DyehLogProvider::new(log_dir);
    lazylog_framework::start_with_provider(&mut terminal, provider)
}
```

**Dependencies:**
- `lazylog-framework` (path dependency)
- `lazylog-dyeh` (path dependency)
- `ratatui`, `crossterm`, `dirs`

**Publishing:** `publish = false` 🔒

## Building

```bash
# Build everything
cargo build --workspace

# Build just the binary
cargo build --package lazylog

# Build just the framework (for publishing)
cargo build --package lazylog-framework

# Check all crates
cargo check --workspace

# Test all crates
cargo test --workspace
```

## Publishing Workflow

### Publishing the Framework

```bash
cd lazylog-framework
cargo publish --dry-run
cargo publish
```

### Using the Published Framework

External users can then:

```toml
# Cargo.toml
[dependencies]
lazylog-framework = "0.1"
```

```rust
use lazylog_framework::{LogProvider, LogItem, start_with_provider};

struct MyLogProvider;

impl LogProvider for MyLogProvider {
    fn start(&mut self) -> anyhow::Result<()> { Ok(()) }
    fn stop(&mut self) -> anyhow::Result<()> { Ok(()) }
    fn poll_logs(&mut self) -> anyhow::Result<Vec<LogItem>> {
        // Your implementation
        Ok(vec![])
    }
}

fn main() {
    let mut terminal = /* setup terminal */;
    let provider = MyLogProvider;
    lazylog_framework::start_with_provider(&mut terminal, provider).unwrap();
}
```

## Development Workflow

### Adding Features to Framework

1. Edit code in `lazylog-framework/src/`
2. Test with: `cargo check --package lazylog-framework`
3. Verify binary still works: `cargo build --package lazylog`

### Adding DYEH-Specific Features

1. Edit code in `lazylog-dyeh/src/`
2. Test with: `cargo check --package lazylog-dyeh`
3. Verify binary: `cargo build --package lazylog`

### Updating the Binary

1. Edit `lazylog-bin/src/main.rs`
2. Build: `cargo build --package lazylog`
3. Run: `cargo run --package lazylog`

## CI/CD Recommendations

### For Open Source (GitHub Actions)

```yaml
# .github/workflows/publish-framework.yml
name: Publish Framework
on:
  push:
    tags:
      - 'framework-v*'
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Publish lazylog-framework
        working-directory: ./lazylog-framework
        run: |
          cargo publish --token ${{ secrets.CARGO_TOKEN }}
```

### For Internal (ByteDance CI)

```yaml
# Build and distribute internal binary
- name: Build Internal Binary
  run: cargo build --package lazylog --release
- name: Upload Artifact
  uses: actions/upload-artifact@v3
  with:
    name: lazylog-macos
    path: target/release/lazylog
```

## Versioning Strategy

- **`lazylog-framework`**: Semver, public releases (e.g., 0.1.0, 0.2.0)
- **`lazylog-dyeh`**: Internal versioning, no public releases
- **`lazylog` binary**: Internal versioning, matches framework

## Migration Notes

### What Changed

1. ✅ Framework is now a standalone crate
2. ✅ DYEH code moved to separate crate
3. ✅ Binary is minimal glue code
4. ✅ All functionality preserved

### What Stayed the Same

- ✅ User-facing behavior unchanged
- ✅ Same keybindings
- ✅ Same UI appearance
- ✅ Same DYEH log support

### Breaking Changes

- ❌ `start()` and `start_with_desc()` removed from framework
- ✅ Replaced with `start_with_provider()` (generic)
- Internal code must now provide a `LogProvider` instance

## Future Enhancements

### Framework

- [ ] Add example providers to `examples/`
- [ ] Write comprehensive README
- [ ] Add API documentation
- [ ] Create tutorial
- [ ] Add more configuration options to `AppDesc`
- [ ] Support custom keybindings
- [ ] Plugin system for event matchers

### DYEH Provider

- [ ] Add configuration for log directory
- [ ] Support multiple log sources
- [ ] Add log file filtering options
- [ ] Performance optimizations

## Benefits of This Structure

✅ **Framework is publishable** - No internal code dependencies
✅ **Clear separation** - Framework vs implementation
✅ **Easy to maintain** - Changes are isolated
✅ **External contributions** - Others can improve framework
✅ **Internal flexibility** - DYEH code can evolve independently
✅ **Reusability** - Framework can power multiple log viewers

## Questions?

- **Q: Can the framework run without DYEH?**
  A: Yes! Just implement `LogProvider` for your log source.

- **Q: Can we have multiple DYEH providers?**
  A: Yes! Create `lazylog-dyeh-v2` alongside `lazylog-dyeh`.

- **Q: How do we update the internal binary?**
  A: Just update `lazylog-bin/Cargo.toml` dependencies.

- **Q: What about breaking changes to framework?**
  A: Use semver. Major version = breaking changes.
