mod agent;
mod mobile_picker;

use agent::{AgentOptions, run_agent};
use crossterm::event;
use lazylog_android::{AndroidEffectParser, AndroidLogProvider, default_device_serial};
use lazylog_dyeh::{DyehEditorParser, DyehLogProvider, DyehParser};
use lazylog_framework::provider::{LogItem, LogParser, LogProvider, ProviderStatus};
use lazylog_framework::{AppDesc, AppExitReason, start_with_desc_until_provider_disconnect};
use lazylog_ios::{IosEffectParser, IosLogProvider, default_device_identifier};
use mobile_picker::{IosApp, PickerSelection, PickerState, ProviderKind};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        style::{Color, ResetColor, SetBackgroundColor, SetForegroundColor},
        terminal::{
            Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
            enable_raw_mode,
        },
    },
};
use std::env;
use std::io;
use std::panic;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn print_usage() {
    eprintln!("Usage: lazylog [OPTIONS]");
    eprintln!("Without a provider option, the TUI opens the Provider picker.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --dyeh-preview          Use DYEH file-based log provider");
    eprintln!("  --dyeh-editor           Use DYEH editor log provider");
    eprintln!("  --ios                   Use iOS app-console provider [EFFECT MODE]");
    eprintln!("  --ios-app <APP>         iOS app: effectcam or douyin");
    eprintln!("  --android               Use Android log provider [EFFECT MODE]");
    eprintln!("  --headless              Stream logs to stdout without the TUI");
    eprintln!(
        "  --agent                 Capture all logs to a unique temporary file (stdout stays empty)"
    );
    eprintln!("  --duration <SECONDS>    Stop agent capture after the given duration");
    eprintln!("  --filter, -f <QUERY>    Apply filter on startup (TUI/headless only)");
    eprintln!("  --version, -v           Print version information");
    eprintln!("  --help, -h              Print this help message");
}

fn agent_usage() -> &'static str {
    r#"Usage: lazylog --agent <PROVIDER> [OPTIONS]

Providers:
  --dyeh-preview          Use DYEH file-based log provider
  --dyeh-editor           Use DYEH editor log provider
  --ios                   Use iOS app-console provider [EFFECT MODE]
  --android               Use Android log provider [EFFECT MODE]

Options:
  --ios-app <APP>         Required with --ios: effectcam or douyin
  --duration <SECONDS>    Stop capture after the given duration
  --help, -h              Print this help message

Output:
  All parsed logs go to a unique temporary file.
  The capture path, status, and final statistics go to stderr; stdout stays empty.
"#
}

fn print_agent_usage() {
    eprint!("{}", agent_usage());
}

fn check_devicectl_available() -> io::Result<()> {
    if !Path::new("/usr/bin/script").is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Error: '/usr/bin/script' was not found. It is required to attach devicectl's console.",
        ));
    }

    match Command::new("xcrun").args(["--find", "devicectl"]).output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Error: 'devicectl' was not found in the selected Xcode.\n\
             Install or select a current Xcode to use iOS modes.",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Error: 'xcrun' was not found. Install Xcode to use iOS modes.",
        )),
        Err(e) => Err(e),
    }
}

fn check_adb_available() -> io::Result<()> {
    // try to execute adb version to check if it's available
    match Command::new("adb").arg("version").output() {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Error: 'adb' not found in PATH.\n\
                 \n\
                 To use Android log provider (--android), you need to install Android SDK Platform-Tools.\n\
                 \n\
                 Installation instructions:\n\
                 - macOS: brew install android-platform-tools\n\
                 - Linux: apt-get install android-tools-adb (Ubuntu/Debian)\n\
                 - Linux: yum install android-tools (CentOS/RHEL)\n\
                 - Windows: Download from https://developer.android.com/studio/releases/platform-tools\n\
                 \n\
                 For more information, visit: https://developer.android.com/studio/command-line/adb",
        )),
        Err(e) => Err(e),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UsageOptions {
    DyehPreview,
    DyehEditor,
    IosEffect,
    AndroidEffect,
    Help,
    Version,
    None, // no explicit provider: open the interactive Provider picker
}

impl UsageOptions {
    fn provider(self) -> Option<ProviderKind> {
        match self {
            Self::IosEffect => Some(ProviderKind::Ios),
            Self::AndroidEffect => Some(ProviderKind::Android),
            Self::DyehPreview => Some(ProviderKind::DyehPreview),
            Self::DyehEditor => Some(ProviderKind::DyehEditor),
            Self::Help | Self::Version | Self::None => None,
        }
    }
}

fn get_mode_name(option: &UsageOptions) -> Option<String> {
    option
        .provider()
        .map(|provider| provider.mode_name().to_string())
}

fn set_provider_option(
    current: &mut UsageOptions,
    new_value: UsageOptions,
) -> Result<(), io::Error> {
    if matches!(current, UsageOptions::None | UsageOptions::Help) {
        *current = new_value;
        return Ok(());
    }

    if *current == new_value {
        return Ok(());
    }

    print_usage();
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "Only one provider option can be used at a time",
    ))
}

struct CliOptions {
    usage_option: UsageOptions,
    headless: bool,
    agent: Option<AgentOptions>,
    initial_filter: Option<String>,
    ios_app: Option<IosApp>,
}

fn take_option_value<'a>(
    args: &'a [String],
    index: &mut usize,
    option: &str,
) -> Result<&'a str, io::Error> {
    *index += 1;
    args.get(*index).map(String::as_str).ok_or_else(|| {
        print_usage();
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Missing value after {option}"),
        )
    })
}

fn duplicate_option(option: &str) -> io::Error {
    print_usage();
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{option} provided multiple times"),
    )
}

impl CliOptions {
    fn from_args(args: &[String]) -> Result<Self, io::Error> {
        let mut usage_option = UsageOptions::None;
        let mut headless = false;
        let mut agent_requested = false;
        let mut duration_seconds = None;
        let mut initial_filter = None;
        let mut ios_app = None;
        let mut help_requested = false;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--ios" => set_provider_option(&mut usage_option, UsageOptions::IosEffect)?,
                "--ios-app" => {
                    let value = take_option_value(args, &mut i, "--ios-app")?;
                    if ios_app.replace(IosApp::parse(value)?).is_some() {
                        return Err(duplicate_option("--ios-app"));
                    }
                }
                "--android" => set_provider_option(&mut usage_option, UsageOptions::AndroidEffect)?,
                "--dyeh-preview" => {
                    set_provider_option(&mut usage_option, UsageOptions::DyehPreview)?
                }
                "--dyeh-editor" => {
                    set_provider_option(&mut usage_option, UsageOptions::DyehEditor)?
                }
                "--version" | "-v" => {
                    set_provider_option(&mut usage_option, UsageOptions::Version)?
                }
                "--filter" | "-f" => {
                    i += 1;
                    if i >= args.len() {
                        print_usage();
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Missing filter value after --filter/-f",
                        ));
                    }
                    if initial_filter.is_some() {
                        print_usage();
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Filter provided multiple times",
                        ));
                    }
                    initial_filter = Some(args[i].clone());
                }
                "--headless" => headless = true,
                "--agent" => agent_requested = true,
                "--capture-file" | "--preview-lines" | "--preview-bytes" => {
                    print_usage();
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "{} was removed; --agent always writes complete logs to a unique temporary file and never writes logs to stdout",
                            args[i]
                        ),
                    ));
                }
                "--duration" => {
                    let value = take_option_value(args, &mut i, "--duration")?;
                    let value = value.parse::<u64>().map_err(|_| {
                        print_usage();
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("Invalid value for --duration: {value}"),
                        )
                    })?;
                    if value == 0 {
                        print_usage();
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "--duration must be greater than zero",
                        ));
                    }
                    if duration_seconds.replace(value).is_some() {
                        return Err(duplicate_option("--duration"));
                    }
                }
                "--help" | "-h" => help_requested = true,
                _ => {
                    print_usage();
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("Unknown option: {}", args[i]),
                    ));
                }
            }
            i += 1;
        }

        if help_requested {
            usage_option = UsageOptions::Help;
        } else {
            if ios_app.is_some() && !matches!(usage_option, UsageOptions::IosEffect) {
                print_usage();
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--ios-app requires --ios",
                ));
            }

            if headless && agent_requested {
                print_usage();
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--headless and --agent cannot be used together",
                ));
            }

            if agent_requested && initial_filter.is_some() {
                print_agent_usage();
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--filter is not supported with --agent; capture all logs, then search the temporary capture file",
                ));
            }

            if (headless || agent_requested) && matches!(usage_option, UsageOptions::None) {
                print_usage();
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--headless and --agent require an explicit provider option",
                ));
            }

            if matches!(usage_option, UsageOptions::IosEffect)
                && (headless || agent_requested)
                && ios_app.is_none()
            {
                print_usage();
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--ios-app is required with --agent or --headless in iOS mode",
                ));
            }

            if duration_seconds.is_some() && !agent_requested {
                print_usage();
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--duration requires --agent",
                ));
            }
        }

        let agent = agent_requested.then(|| AgentOptions {
            duration: duration_seconds.map(Duration::from_secs),
        });

        Ok(Self {
            usage_option,
            headless,
            agent,
            initial_filter,
            ios_app,
        })
    }
}

fn make_ios_provider(device: &str, app: IosApp) -> IosLogProvider {
    IosLogProvider::new_app_console(device, app.bundle_id())
}

fn build_app_desc(
    parser: Arc<dyn LogParser>,
    option: &UsageOptions,
    initial_filter: &Option<String>,
    poll_interval: Duration,
    ios_app: Option<IosApp>,
) -> AppDesc {
    let mut desc = AppDesc::new(parser);
    desc.initial_filter = initial_filter.clone();
    desc.poll_interval = poll_interval;
    desc.mode_name = get_mode_name(option);
    if let Some(app) = ios_app {
        desc.mode_name = desc
            .mode_name
            .map(|name| format!("{name} app console ({})", app.display_name()));
    }
    desc
}

fn run_interactive_session(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    selection: &PickerSelection,
    initial_filter: &Option<String>,
    poll_interval: Duration,
) -> anyhow::Result<AppExitReason> {
    match selection {
        PickerSelection::Ios(selection) => {
            check_devicectl_available()?;
            let parser: Arc<dyn LogParser> = Arc::new(IosEffectParser::new());
            let desc = build_app_desc(
                parser,
                &UsageOptions::IosEffect,
                initial_filter,
                poll_interval,
                Some(selection.app),
            );
            start_with_desc_until_provider_disconnect(
                terminal,
                make_ios_provider(&selection.device, selection.app),
                desc,
            )
        }
        PickerSelection::Android { device } => {
            check_adb_available()?;
            let parser: Arc<dyn LogParser> = Arc::new(AndroidEffectParser::new());
            let desc = build_app_desc(
                parser,
                &UsageOptions::AndroidEffect,
                initial_filter,
                poll_interval,
                None,
            );
            start_with_desc_until_provider_disconnect(
                terminal,
                AndroidLogProvider::new_for_device(device.clone()),
                desc,
            )
        }
        PickerSelection::DyehPreview => {
            let log_dir_path = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
                .join("Library/Application Support/DouyinAR");
            let parser: Arc<dyn LogParser> = Arc::new(DyehParser::new());
            let desc = build_app_desc(
                parser,
                &UsageOptions::DyehPreview,
                initial_filter,
                poll_interval,
                None,
            );
            start_with_desc_until_provider_disconnect(
                terminal,
                DyehLogProvider::new(log_dir_path),
                desc,
            )
        }
        PickerSelection::DyehEditor => {
            let log_dir_path = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
                .join("Library/Application Support/DouyinAR");
            let parser: Arc<dyn LogParser> = Arc::new(DyehEditorParser::new());
            let desc = build_app_desc(
                parser,
                &UsageOptions::DyehEditor,
                initial_filter,
                poll_interval,
                None,
            );
            start_with_desc_until_provider_disconnect(
                terminal,
                DyehLogProvider::new_editor(log_dir_path),
                desc,
            )
        }
    }
}

fn initial_selection(provider: Option<ProviderKind>) -> Option<PickerSelection> {
    match provider {
        Some(ProviderKind::DyehPreview) => Some(PickerSelection::DyehPreview),
        Some(ProviderKind::DyehEditor) => Some(PickerSelection::DyehEditor),
        Some(ProviderKind::Ios | ProviderKind::Android) | None => None,
    }
}

fn run_interactive(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    requested_provider: Option<ProviderKind>,
    requested_app: Option<IosApp>,
    initial_filter: &Option<String>,
    poll_interval: Duration,
) -> anyhow::Result<()> {
    let mut picker = PickerState::new(requested_provider, requested_app);
    let mut pending_selection = initial_selection(requested_provider);

    loop {
        let selection = if let Some(selection) = pending_selection.take() {
            selection
        } else {
            let Some(selection) = mobile_picker::pick(terminal, &mut picker)? else {
                return Ok(());
            };
            selection
        };

        match run_interactive_session(terminal, &selection, initial_filter, poll_interval) {
            Ok(AppExitReason::UserQuit) => return Ok(()),
            Ok(AppExitReason::UserBack | AppExitReason::ProviderDisconnected(_)) => {}
            Err(error) => picker.set_message(format!("启动失败：{error}")),
        }
    }
}

fn matches_filter(
    parser: &Arc<dyn LogParser>,
    item: &LogItem,
    initial_filter: Option<&str>,
) -> bool {
    let Some(query) = initial_filter
        .map(str::trim)
        .filter(|query| !query.is_empty())
    else {
        return true;
    };

    parser
        .get_searchable_text(item, parser.max_detail_level())
        .to_lowercase()
        .contains(&query.to_lowercase())
}

fn run_headless<P>(
    mut provider: P,
    parser: Arc<dyn LogParser>,
    initial_filter: Option<&str>,
    poll_interval: Duration,
) -> io::Result<()>
where
    P: LogProvider,
{
    provider.start().map_err(io::Error::other)?;
    let mut provider_status = None;

    loop {
        match provider.poll_logs() {
            Ok(raw_logs) => {
                for raw_log in raw_logs {
                    if let Some(item) = parser.parse(&raw_log)
                        && matches_filter(&parser, &item, initial_filter)
                    {
                        let color = get_headless_log_color(&item);
                        println!(
                            "{}{}{}",
                            SetForegroundColor(color),
                            item.raw_content,
                            ResetColor
                        );
                    }
                }
            }
            Err(err) => eprintln!("Provider poll error: {}", err),
        }

        report_provider_status(&provider, &mut provider_status);

        thread::sleep(poll_interval);
    }
}

fn report_provider_status<P: LogProvider>(provider: &P, previous: &mut Option<ProviderStatus>) {
    let current = provider.status();
    if current != *previous {
        if let Some(status) = current {
            eprintln!("[lazylog] 连接状态: {}", status.label());
        }
        *previous = current;
    }
}

fn run_noninteractive<P>(
    provider: P,
    parser: Arc<dyn LogParser>,
    initial_filter: Option<&str>,
    poll_interval: Duration,
    agent_options: Option<&AgentOptions>,
) -> io::Result<()>
where
    P: LogProvider,
{
    match agent_options {
        Some(options) => run_agent(provider, parser, poll_interval, options),
        None => run_headless(provider, parser, initial_filter, poll_interval),
    }
}

fn get_headless_log_color(item: &LogItem) -> Color {
    let level = item.get_metadata("level").unwrap_or("").to_uppercase();
    match level.as_str() {
        "ERROR" => Color::Red,
        "WARNING" | "WARN" => Color::Yellow,
        "SYSTEM" => Color::White,
        _ => Color::Grey,
    }
}

fn main() -> io::Result<()> {
    // Collect args excluding the binary name
    let args: Vec<String> = env::args().skip(1).collect();
    let cli_options = CliOptions::from_args(&args)?;
    let usage_option = cli_options.usage_option;

    if matches!(usage_option, UsageOptions::Version) {
        println!("lazylog {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if matches!(usage_option, UsageOptions::Help) {
        if cli_options.agent.is_some() {
            print_agent_usage();
        } else {
            print_usage();
        }
        return Ok(());
    }

    let poll_interval = Duration::from_millis(20);

    if cli_options.headless || cli_options.agent.is_some() {
        if matches!(usage_option, UsageOptions::IosEffect) {
            check_devicectl_available()?;
        }
        if matches!(usage_option, UsageOptions::AndroidEffect) {
            check_adb_available()?;
        }
        let initial_filter = cli_options.initial_filter.as_deref();
        let agent_options = cli_options.agent.as_ref();
        return match usage_option {
            UsageOptions::IosEffect => {
                let device = default_device_identifier().map_err(io::Error::other)?;
                let app = cli_options
                    .ios_app
                    .expect("non-interactive iOS mode requires --ios-app");
                run_noninteractive(
                    make_ios_provider(&device, app),
                    Arc::new(IosEffectParser::new()),
                    initial_filter,
                    poll_interval,
                    agent_options,
                )
            }
            UsageOptions::AndroidEffect => {
                let device = default_device_serial().map_err(io::Error::other)?;
                run_noninteractive(
                    AndroidLogProvider::new_for_device(device),
                    Arc::new(AndroidEffectParser::new()),
                    initial_filter,
                    poll_interval,
                    agent_options,
                )
            }
            UsageOptions::DyehPreview => {
                if let Some(dir) = dirs::home_dir() {
                    let log_dir_path = dir.join("Library/Application Support/DouyinAR");
                    run_noninteractive(
                        DyehLogProvider::new(log_dir_path),
                        Arc::new(DyehParser::new()),
                        initial_filter,
                        poll_interval,
                        agent_options,
                    )
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "Error: Could not determine home directory",
                    ))
                }
            }
            UsageOptions::DyehEditor => {
                if let Some(dir) = dirs::home_dir() {
                    let log_dir_path = dir.join("Library/Application Support/DouyinAR");
                    run_noninteractive(
                        DyehLogProvider::new_editor(log_dir_path),
                        Arc::new(DyehEditorParser::new()),
                        initial_filter,
                        poll_interval,
                        agent_options,
                    )
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "Error: Could not determine home directory",
                    ))
                }
            }
            UsageOptions::Help | UsageOptions::None | UsageOptions::Version => unreachable!(),
        };
    }

    let mut terminal = setup_terminal()?;

    // Ensure we restore the terminal on panic
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        original_hook(panic_info);
    }));

    let initial_filter = cli_options.initial_filter;

    let app_result = run_interactive(
        &mut terminal,
        usage_option.provider(),
        cli_options.ios_app,
        &initial_filter,
        poll_interval,
    );

    // Always restore terminal before printing or exiting
    restore_terminal()?;

    if let Err(err) = app_result {
        eprintln!("Application Error: {:?}", err);
    }

    Ok(())
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // enter the alternate screen to not mess with the user's shell history
    // enable mouse capture to receive mouse events
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    // force reset background color to black and clear the screen
    execute!(
        stdout,
        SetBackgroundColor(Color::Reset),
        Clear(ClearType::All)
    )?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal() -> io::Result<()> {
    let mut stdout = io::stdout();

    // reset colors before leaving
    let _ = execute!(stdout, ResetColor);
    // Best-effort cleanup; ignore errors during teardown where sensible
    let _ = execute!(stdout, DisableMouseCapture);
    let _ = execute!(stdout, LeaveAlternateScreen);

    // Drain pending events so they don't leak to the shell
    while event::poll(Duration::from_millis(0)).unwrap_or(false) {
        let _ = event::read();
    }

    let _ = disable_raw_mode();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn agent_options_default_to_unbounded_duration() {
        let options = CliOptions::from_args(&args(&["--agent", "--dyeh-preview"])).unwrap();
        let agent = options.agent.unwrap();

        assert!(!options.headless);
        assert!(agent.duration.is_none());
    }

    #[test]
    fn agent_options_accept_duration() {
        let options =
            CliOptions::from_args(&args(&["--agent", "--dyeh-editor", "--duration", "6"])).unwrap();
        let agent = options.agent.unwrap();

        assert_eq!(agent.duration, Some(Duration::from_secs(6)));
    }

    #[test]
    fn duration_requires_agent_mode() {
        let error =
            CliOptions::from_args(&args(&["--headless", "--dyeh-preview", "--duration", "10"]))
                .err()
                .unwrap();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("--duration requires --agent"));
    }

    #[test]
    fn agent_rejects_filter_in_either_argument_order() {
        for values in [
            ["--agent", "--dyeh-preview", "--filter", "ERROR"],
            ["--filter", "ERROR", "--agent", "--dyeh-preview"],
        ] {
            let error = CliOptions::from_args(&args(&values)).err().unwrap();

            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert!(
                error
                    .to_string()
                    .contains("--filter is not supported with --agent")
            );
        }
    }

    #[test]
    fn agent_help_exposes_only_agent_options() {
        let usage = agent_usage();

        assert!(usage.contains("lazylog --agent <PROVIDER>"));
        assert!(usage.contains("--duration"));
        assert!(usage.contains("stdout stays empty"));
        assert!(!usage.contains("--filter"));
        assert!(!usage.contains("--headless"));
    }

    #[test]
    fn user_modes_keep_filter_support() {
        let interactive = CliOptions::from_args(&args(&["--filter", "ERROR"])).unwrap();
        assert_eq!(interactive.initial_filter.as_deref(), Some("ERROR"));

        let headless = CliOptions::from_args(&args(&[
            "--headless",
            "--dyeh-preview",
            "--filter",
            "WARNING",
        ]))
        .unwrap();
        assert_eq!(headless.initial_filter.as_deref(), Some("WARNING"));
    }

    #[test]
    fn removed_agent_output_options_explain_temporary_capture() {
        for option in ["--capture-file", "--preview-lines", "--preview-bytes"] {
            let error = CliOptions::from_args(&args(&["--agent", "--dyeh-preview", option]))
                .err()
                .unwrap();

            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert!(error.to_string().contains("was removed"));
            assert!(error.to_string().contains("temporary file"));
        }
    }

    #[test]
    fn agent_and_headless_are_mutually_exclusive() {
        let error = CliOptions::from_args(&args(&["--headless", "--agent", "--dyeh-preview"]))
            .err()
            .unwrap();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("cannot be used together"));
    }

    #[test]
    fn no_arguments_open_the_interactive_provider_picker() {
        let options = CliOptions::from_args(&args(&[])).unwrap();

        assert_eq!(options.usage_option, UsageOptions::None);
        assert!(!options.headless);
        assert!(options.agent.is_none());
    }

    #[test]
    fn noninteractive_modes_require_an_explicit_provider() {
        for mode in ["--agent", "--headless"] {
            let error = CliOptions::from_args(&args(&[mode])).err().unwrap();
            assert!(error.to_string().contains("explicit provider"));
        }
    }

    #[test]
    fn provider_short_options_are_not_accepted() {
        for option in [
            "-i",
            "-ie",
            "-a",
            "-ae",
            "-dyp",
            "-dye",
            "--ios-effect",
            "--android-effect",
        ] {
            let error = CliOptions::from_args(&args(&[option])).err().unwrap();
            assert!(error.to_string().contains("Unknown option"));
        }
    }

    #[test]
    fn long_provider_options_map_to_picker_providers() {
        for (option, provider) in [
            ("--ios", ProviderKind::Ios),
            ("--android", ProviderKind::Android),
            ("--dyeh-preview", ProviderKind::DyehPreview),
            ("--dyeh-editor", ProviderKind::DyehEditor),
        ] {
            let options = CliOptions::from_args(&args(&[option])).unwrap();
            assert_eq!(options.usage_option.provider(), Some(provider));
        }
    }

    #[test]
    fn interactive_ios_mode_has_no_default_app() {
        let options = CliOptions::from_args(&args(&["--ios"])).unwrap();

        assert_eq!(options.ios_app, None);
    }

    #[test]
    fn ios_app_selects_douyin() {
        let options = CliOptions::from_args(&args(&["--ios", "--ios-app", "douyin"])).unwrap();

        assert_eq!(options.ios_app, Some(IosApp::Douyin));
        assert_eq!(
            options.ios_app.unwrap().bundle_id(),
            IosApp::DOUYIN_BUNDLE_ID
        );
    }

    #[test]
    fn agent_ios_mode_requires_an_explicit_app() {
        let error = CliOptions::from_args(&args(&["--agent", "--ios"]))
            .err()
            .unwrap();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("--ios-app is required"));
    }

    #[test]
    fn headless_ios_mode_requires_an_explicit_app() {
        let error = CliOptions::from_args(&args(&["--headless", "--ios"]))
            .err()
            .unwrap();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("--ios-app is required"));
    }

    #[test]
    fn ios_app_requires_an_ios_mode() {
        let error = CliOptions::from_args(&args(&["--android", "--ios-app", "douyin"]))
            .err()
            .unwrap();

        assert!(error.to_string().contains("requires --ios"));
    }

    #[test]
    fn ios_app_rejects_unknown_alias() {
        let error = CliOptions::from_args(&args(&["--ios", "--ios-app", "unknown"]))
            .err()
            .unwrap();

        assert!(error.to_string().contains("expected effectcam or douyin"));
    }
}
