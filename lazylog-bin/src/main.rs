use crossterm::event;
use lazylog_android::{AndroidEffectParser, AndroidLogProvider, AndroidParser};
use lazylog_dyeh::{DyehEditorParser, DyehLogProvider, DyehParser};
use lazylog_framework::provider::{LogItem, LogParser, LogProvider};
use lazylog_framework::{AppDesc, start_with_desc};
use lazylog_ios::{IosEffectParser, IosFullParser, IosLogProvider};
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
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn print_usage() {
    eprintln!("Usage: lazylog [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --dyeh-preview, -dyp    Use DYEH file-based log provider");
    eprintln!("  --dyeh-editor, -dye     Use DYEH editor log provider");
    eprintln!("  --ios, -i               Use iOS log provider");
    eprintln!("  --ios-effect, -ie       Use iOS log provider [EFFECT MODE]");
    eprintln!("  --android, -a           Use Android log provider");
    eprintln!("  --android-effect, -ae   Use Android log provider [EFFECT MODE]");
    eprintln!("  --headless              Stream logs to stdout without the TUI");
    eprintln!("  --filter, -f <QUERY>    Apply filter on startup");
    eprintln!("  --version, -v           Print version information");
    eprintln!("  --help, -h              Print this help message");
}

fn check_idevicesyslog_available() -> io::Result<()> {
    // try to execute idevicesyslog --version to check if it's available
    match Command::new("idevicesyslog").arg("--version").output() {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Error: 'idevicesyslog' not found in PATH.\n\
                 \n\
                 To use iOS log providers (-i or -ie), you need to install libimobiledevice.\n\
                 \n\
                 Installation instructions:\n\
                 - macOS: brew install libimobiledevice\n\
                 - Linux: apt-get install libimobiledevice-utils (Ubuntu/Debian)\n\
                 - Linux: yum install libimobiledevice (CentOS/RHEL)\n\
                 \n\
                 For more information, visit: https://libimobiledevice.org/",
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
                 To use Android log provider (-a or --android), you need to install Android SDK Platform-Tools.\n\
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

#[derive(PartialEq, Eq)]
enum UsageOptions {
    DyehPreview,
    DyehEditor,
    IosEffect,
    IosFull,
    Android,
    AndroidEffect,
    Help,
    Version,
    None, // when no args provided, show help
}

fn get_mode_name(option: &UsageOptions) -> Option<String> {
    use UsageOptions::*;
    match option {
        DyehPreview => Some("dyeh preview".to_string()),
        DyehEditor => Some("dyeh editor".to_string()),
        IosEffect => Some("ios effect".to_string()),
        IosFull => Some("ios".to_string()),
        Android => Some("android".to_string()),
        AndroidEffect => Some("android effect".to_string()),
        Help | Version | None => Option::None,
    }
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
    initial_filter: Option<String>,
}

impl CliOptions {
    fn from_args(args: &[String]) -> Result<Self, io::Error> {
        let mut usage_option = UsageOptions::None;
        let mut headless = false;
        let mut initial_filter = None;
        let mut help_requested = false;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--ios-effect" | "-ie" => {
                    set_provider_option(&mut usage_option, UsageOptions::IosEffect)?
                }
                "--ios" | "-i" => set_provider_option(&mut usage_option, UsageOptions::IosFull)?,
                "--android" | "-a" => {
                    set_provider_option(&mut usage_option, UsageOptions::Android)?
                }
                "--android-effect" | "-ae" => {
                    set_provider_option(&mut usage_option, UsageOptions::AndroidEffect)?
                }
                "--dyeh-preview" | "-dyp" => {
                    set_provider_option(&mut usage_option, UsageOptions::DyehPreview)?
                }
                "--dyeh-editor" | "-dye" => {
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
        }

        Ok(Self {
            usage_option,
            headless,
            initial_filter,
        })
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

    loop {
        match provider.poll_logs() {
            Ok(raw_logs) => {
                for raw_log in raw_logs {
                    if let Some(item) = parser.parse(&raw_log)
                        && matches_filter(&parser, &item, initial_filter)
                    {
                        let color = get_headless_log_color(&item);
                        print!(
                            "{}{}{}\n",
                            SetForegroundColor(color),
                            item.raw_content,
                            ResetColor
                        );
                    }
                }
            }
            Err(err) => eprintln!("Provider poll error: {}", err),
        }

        thread::sleep(poll_interval);
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

    if matches!(usage_option, UsageOptions::Help | UsageOptions::None) {
        print_usage();
        return Ok(());
    }

    let poll_interval = Duration::from_millis(20);
    // check if idevicesyslog is available for iOS options
    if matches!(
        usage_option,
        UsageOptions::IosEffect | UsageOptions::IosFull
    ) && let Err(e) = check_idevicesyslog_available()
    {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    // check if adb is available for Android option
    if matches!(
        usage_option,
        UsageOptions::Android | UsageOptions::AndroidEffect
    ) && let Err(e) = check_adb_available()
    {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    if cli_options.headless {
        let initial_filter = cli_options.initial_filter.as_deref();
        return match usage_option {
            UsageOptions::IosEffect => run_headless(
                IosLogProvider::new(),
                Arc::new(IosEffectParser::new()),
                initial_filter,
                poll_interval,
            ),
            UsageOptions::IosFull => run_headless(
                IosLogProvider::new(),
                Arc::new(IosFullParser::new()),
                initial_filter,
                poll_interval,
            ),
            UsageOptions::Android => run_headless(
                AndroidLogProvider::new(),
                Arc::new(AndroidParser::new()),
                initial_filter,
                poll_interval,
            ),
            UsageOptions::AndroidEffect => run_headless(
                AndroidLogProvider::new(),
                Arc::new(AndroidEffectParser::new()),
                initial_filter,
                poll_interval,
            ),
            UsageOptions::DyehPreview => {
                if let Some(dir) = dirs::home_dir() {
                    let log_dir_path = dir.join("Library/Application Support/DouyinAR");
                    run_headless(
                        DyehLogProvider::new(log_dir_path),
                        Arc::new(DyehParser::new()),
                        initial_filter,
                        poll_interval,
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
                    run_headless(
                        DyehLogProvider::new_editor(log_dir_path),
                        Arc::new(DyehEditorParser::new()),
                        initial_filter,
                        poll_interval,
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

    let build_desc = |parser: Arc<dyn lazylog_framework::provider::LogParser>,
                      option: UsageOptions|
     -> AppDesc {
        let mut desc = AppDesc::new(parser);
        desc.initial_filter = initial_filter.clone();
        desc.poll_interval = poll_interval;
        desc.mode_name = get_mode_name(&option);
        desc
    };

    // Prepare provider and parser based on option (default to DYEH)
    let app_result = match usage_option {
        UsageOptions::IosEffect => {
            let provider = IosLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(IosEffectParser::new());
            let desc = build_desc(parser, UsageOptions::IosEffect);
            start_with_desc(&mut terminal, provider, desc)
        }
        UsageOptions::IosFull => {
            let provider = IosLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(IosFullParser::new());
            let desc = build_desc(parser, UsageOptions::IosFull);
            start_with_desc(&mut terminal, provider, desc)
        }
        UsageOptions::Android => {
            let provider = AndroidLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(AndroidParser::new());
            let desc = build_desc(parser, UsageOptions::Android);
            start_with_desc(&mut terminal, provider, desc)
        }
        UsageOptions::AndroidEffect => {
            let provider = AndroidLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(AndroidEffectParser::new());
            let desc = build_desc(parser, UsageOptions::AndroidEffect);
            start_with_desc(&mut terminal, provider, desc)
        }
        UsageOptions::DyehPreview => {
            if let Some(dir) = dirs::home_dir() {
                let log_dir_path = dir.join("Library/Application Support/DouyinAR");
                let provider = DyehLogProvider::new(log_dir_path);
                let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                    Arc::new(DyehParser::new());
                let desc = build_desc(parser, UsageOptions::DyehPreview);
                start_with_desc(&mut terminal, provider, desc)
            } else {
                eprintln!("Error: Could not determine home directory");
                Ok(())
            }
        }
        UsageOptions::DyehEditor => {
            if let Some(dir) = dirs::home_dir() {
                let log_dir_path = dir.join("Library/Application Support/DouyinAR");
                let provider = DyehLogProvider::new_editor(log_dir_path);
                let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                    Arc::new(DyehEditorParser::new());
                let desc = build_desc(parser, UsageOptions::DyehEditor);
                start_with_desc(&mut terminal, provider, desc)
            } else {
                eprintln!("Error: Could not determine home directory");
                Ok(())
            }
        }
        UsageOptions::Help | UsageOptions::None | UsageOptions::Version => unreachable!(),
    };

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
