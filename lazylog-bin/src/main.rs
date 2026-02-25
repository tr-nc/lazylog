use crossterm::event;
use lazylog_android::{AndroidEffectParser, AndroidLogProvider, AndroidParser};
use lazylog_dyeh::{DyehLogProvider, DyehParser};
use lazylog_framework::{start_with_desc, AppDesc};
use lazylog_ios::{IosEffectParser, IosFullParser, IosLogProvider};
use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        style::{Color, ResetColor, SetBackgroundColor},
        terminal::{
            disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
            LeaveAlternateScreen,
        },
    },
    Terminal,
};
use std::env;
use std::io;
use std::panic;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

fn print_usage() {
    eprintln!("Usage: lazylog [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --dyeh, -dy             Use DYEH file-based log provider");
    eprintln!("  --ios, -i               Use iOS log provider");
    eprintln!("  --ios-effect, -ie       Use iOS log provider [EFFECT MODE]");
    eprintln!("  --android, -a           Use Android log provider");
    eprintln!("  --android-effect, -ae   Use Android log provider [EFFECT MODE]");
    eprintln!("  --filter, -f <QUERY>    Apply filter on startup");
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
    IosEffect,
    IosFull,
    Android,
    AndroidEffect,
    Dyeh,
    Help,
    None, // when no args provided, show help
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
    initial_filter: Option<String>,
}

impl CliOptions {
    fn from_args(args: &[String]) -> Result<Self, io::Error> {
        let mut usage_option = UsageOptions::None;
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
                "--dyeh" | "-dy" => set_provider_option(&mut usage_option, UsageOptions::Dyeh)?,
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
            initial_filter,
        })
    }
}

fn main() -> io::Result<()> {
    // Collect args excluding the binary name
    let args: Vec<String> = env::args().skip(1).collect();
    let cli_options = CliOptions::from_args(&args)?;
    let usage_option = cli_options.usage_option;

    if matches!(usage_option, UsageOptions::Help | UsageOptions::None) {
        print_usage();
        return Ok(());
    }

    // check if idevicesyslog is available for iOS options
    if matches!(
        usage_option,
        UsageOptions::IosEffect | UsageOptions::IosFull
    ) {
        if let Err(e) = check_idevicesyslog_available() {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }

    // check if adb is available for Android option
    if matches!(
        usage_option,
        UsageOptions::Android | UsageOptions::AndroidEffect
    ) {
        if let Err(e) = check_adb_available() {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }

    let mut terminal = setup_terminal()?;

    // Ensure we restore the terminal on panic
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        original_hook(panic_info);
    }));

    let initial_filter = cli_options.initial_filter;

    let build_desc = |parser: Arc<dyn lazylog_framework::provider::LogParser>| -> AppDesc {
        let mut desc = AppDesc::new(parser);
        desc.initial_filter = initial_filter.clone();
        desc
    };

    // Prepare provider and parser based on option (default to DYEH)
    let app_result = match usage_option {
        UsageOptions::IosEffect => {
            let provider = IosLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(IosEffectParser::new());
            let desc = build_desc(parser);
            start_with_desc(&mut terminal, provider, desc)
        }
        UsageOptions::IosFull => {
            let provider = IosLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(IosFullParser::new());
            let desc = build_desc(parser);
            start_with_desc(&mut terminal, provider, desc)
        }
        UsageOptions::Android => {
            let provider = AndroidLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(AndroidParser::new());
            let desc = build_desc(parser);
            start_with_desc(&mut terminal, provider, desc)
        }
        UsageOptions::AndroidEffect => {
            let provider = AndroidLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(AndroidEffectParser::new());
            let desc = build_desc(parser);
            start_with_desc(&mut terminal, provider, desc)
        }
        UsageOptions::Dyeh => {
            if let Some(dir) = dirs::home_dir() {
                let log_dir_path = dir.join("Library/Application Support/DouyinAR");
                let provider = DyehLogProvider::new(log_dir_path);
                let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                    Arc::new(DyehParser::new());
                let desc = build_desc(parser);
                start_with_desc(&mut terminal, provider, desc)
            } else {
                eprintln!("Error: Could not determine home directory");
                Ok(())
            }
        }
        UsageOptions::Help | UsageOptions::None => unreachable!(),
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
