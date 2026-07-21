---
name: lazylog-headless-debugger
description: Uses lazylog agent and headless modes for non-interactive log debugging across all supported providers.
version: 0.2.0
---

# Lazylog Agent Debugger

Use this skill when you want to debug logs with `lazylog` in a non-interactive way.

## Scope

- Install `lazylog` for end users with Homebrew.
- Use `--agent` for coding-agent-driven debugging with bounded stdout and a complete capture file.
- Use `--headless` when an unbounded stdout stream is explicitly needed by a script.
- Stream logs from any supported provider directly to the terminal.
- Narrow noisy output with `--filter` when needed.

## Installation

Install `lazylog` with Homebrew:

```bash
brew install tr-nc/tap/lazylog
```

Upgrade later with:

```bash
brew upgrade lazylog
```

Platform dependencies:

- iOS modes require `idevicesyslog`
- Android modes require `adb`
- DYEH modes read from the local DYEH log directories

After installation, use the `lazylog` command directly.

## Supported Providers

Agent and headless modes support all current providers:

- `--dyeh-preview`
- `--dyeh-editor`
- `--ios`
- `--ios-effect`
- `--android`
- `--android-effect`

## Workflow

1. Pick the provider that matches the environment you want to debug.
2. Add `--agent`.
3. Add `--filter` if you already know the error keyword or tag you want.
4. Add `--duration` when a bounded capture window is appropriate.
5. Inspect the stdout preview first, then read or search the reported capture path if output was truncated.
6. Stop the command manually when you are done if no duration was provided.

For coding agents, prefer `--agent`. It prevents a noisy stream from consuming the command-output
budget while preserving the complete filtered session on disk.

## Common Commands

Debug DYEH preview logs:

```bash
lazylog --agent --dyeh-preview --duration 30
```

Debug DYEH editor logs:

```bash
lazylog --agent --dyeh-editor --duration 30
```

Debug Android effect logs with a startup filter:

```bash
lazylog --agent --android-effect --filter "ERROR" --duration 30
```

Debug iOS logs non-interactively:

```bash
lazylog --agent --ios --duration 30
```

## Behavior

- agent mode captures until interrupted or until `--duration` expires
- the complete plain-text capture path is printed when the session starts
- stdout preview defaults to 500 lines and 64 KiB
- after either preview limit, capture continues without printing more log items to stdout
- headless mode remains an unbounded colorized stream until interrupted
- `--filter` is applied before printing
- each matching item is printed using full `raw_content`
- agent captures are plain text; headless output is colorized by log level

## Headless Color Rules

- `ERROR` uses red
- `WARNING` and `WARN` use yellow
- `SYSTEM` uses white
- all other logs use gray

## Output Style

- Prefer direct runnable commands.
- Prefer `--agent` over TUI or unbounded headless instructions for coding-agent automation.
- When suggesting a command, include the provider flag explicitly.
- Include `--duration` unless the debugging task specifically needs an open-ended capture.
