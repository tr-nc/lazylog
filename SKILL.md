---
name: lazylog-headless-debugger
description: Uses lazylog agent and headless modes for non-interactive log debugging across all supported providers.
version: 0.3.0
---

# Lazylog Agent Debugger

Use this skill when you want to debug logs with `lazylog` in a non-interactive way.

## Scope

- Install `lazylog` for end users with Homebrew.
- Use `--agent` for coding-agent-driven debugging with an empty stdout and a complete temporary capture file.
- Use `--headless` when an unbounded stdout stream is explicitly needed by a script.
- Stream logs from any supported provider directly to the terminal.
- Narrow TUI or headless output with `--filter` when needed.

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

- iOS mode requires a current Xcode with `devicectl`
- Android modes require `adb`
- DYEH modes read from the local DYEH log directories

After installation, use the `lazylog` command directly.

## Supported Providers

Agent and headless modes support all current providers:

- `--dyeh-preview`
- `--dyeh-editor`
- `--ios`
- `--android`

## Workflow

1. Pick the provider that matches the environment you want to debug.
2. Add `--agent`.
3. Add `--duration` when a bounded capture window is appropriate.
4. Read the capture path reported on stderr.
5. Read or search that file directly, for example with `rg`.
6. Stop the command manually when you are done if no duration was provided.

For coding agents, prefer `--agent`. It prevents a noisy stream from consuming the command-output
budget by writing the complete parsed session to a unique OS temporary file and nothing to stdout.

## Common Commands

Debug DYEH preview logs:

```bash
lazylog --agent --dyeh-preview --duration 30
```

Debug DYEH editor logs:

```bash
lazylog --agent --dyeh-editor --duration 30
```

Debug Android effect logs:

```bash
lazylog --agent --android --duration 30
```

Debug EffectCam iOS effect logs non-interactively:

```bash
lazylog --agent --ios --ios-app effectcam --duration 30
```

Debug Douyin iOS effect logs:

```bash
lazylog --agent --ios --ios-app douyin --duration 30
```

Interactive mobile mode first opens the shared device picker, which refreshes automatically and
highlights the first device. Android proceeds directly to logs and never opens an App picker. iOS
then opens its own App picker when `--ios-app` is omitted. Agent and headless modes have no
interaction: they select the preferred connected device automatically, and iOS must also pass
either `--ios-app effectcam` or `--ios-app douyin`. Lazylog then terminates and relaunches that iOS
App so its standard streams can be attached. Use it only on a development or test device:
unredacted output can contain credentials and user data.

The iOS App picker detects whether each App already has a process, regardless of who launched it.
It cannot distinguish foreground from background/suspended state and therefore labels this as an
existing process rather than saying the App is running. If the highlighted App has a process, the
picker explicitly warns that confirmation will terminate it before relaunching the App for console
attachment. A missing bundle ID is shown as not installed and cannot be confirmed.

## Behavior

- agent mode captures until interrupted or until `--duration` expires
- interactive, headless, and agent modes report the same connection-state labels
- iOS distinguishes USB removal, device disconnect, target App exit, and capture failure
- interactive iOS App exit returns to the iOS-only App picker
- interactive iOS/Android device disconnect returns to the shared device picker
- Android distinguishes device disconnect from capture failure; global `adb logcat` does not
  provide reliable target-App exit detection
- every agent invocation creates a unique plain-text file under the OS temporary directory at
  `lazylog/agent-captures`
- agent mode reports the capture path, status changes, and final statistics on stderr
- agent mode never writes log content to stdout
- agent mode captures every parsed item and rejects `--filter`; search the capture file instead
- headless mode remains an unbounded colorized stream until interrupted
- headless `--filter` is applied before printing
- each matching headless item is printed using full `raw_content`
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
