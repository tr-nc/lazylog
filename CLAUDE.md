# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Rules

If the code is already self-explainable, remove the comments.

For inline comments, don't capitalize the first letter.
  
## Project Overview

Lazylog is a terminal-based log file viewer built with Rust and ratatui. It provides real-time log monitoring with vim-like navigation, structured log parsing, and efficient handling of large files through memory-mapped access.

## Development Commands

### Code Quality

```bash
# Format code
cargo fmt

# Check compilation without building
cargo check
```
