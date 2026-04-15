---
name: lazylog-headless-debugger
description: Uses lazylog in headless mode for non-interactive log debugging across all supported providers.
version: 0.1.0
---

# Lazylog Headless Debugger

Use this skill when you want to debug logs with `lazylog` in a non-interactive way.

## Scope

- Install `lazylog` for end users with Homebrew.
- Use `--headless` for automated or coding-agent-driven debugging.
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

Headless mode supports all current providers:

- `--dyeh-preview`
- `--dyeh-editor`
- `--ios`
- `--ios-effect`
- `--android`
- `--android-effect`

## Workflow

1. Pick the provider that matches the environment you want to debug.
2. Add `--headless`.
3. Add `--filter` if you already know the error keyword or tag you want.
4. Run the command and let it stream until you have enough logs.
5. Stop the command manually when you are done.

For coding agents, you almost always want `--headless`, because agent-driven debugging is usually automated and non-interactive.

## Common Commands

Debug DYEH preview logs:

```bash
lazylog --headless --dyeh-preview
```

Debug DYEH editor logs:

```bash
lazylog --headless --dyeh-editor
```

Debug Android effect logs with a startup filter:

```bash
lazylog --headless --android-effect --filter "ERROR"
```

Debug iOS logs non-interactively:

```bash
lazylog --headless --ios
```

## Behavior

- headless mode streams forever until interrupted
- `--filter` is applied before printing
- each matching item is printed using full `raw_content`
- output is colorized by log level

## Color Rules

- `ERROR` uses red
- `WARNING` and `WARN` use yellow
- `SYSTEM` uses white
- all other logs use gray

## Output Style

- Prefer direct runnable commands.
- Prefer `--headless` over TUI instructions for automation.
- When suggesting a command, include the provider flag explicitly.
