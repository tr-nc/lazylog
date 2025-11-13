use crossterm::event;
use lazylog_android::{AndroidEffectParser, AndroidLogProvider, AndroidParser};
use lazylog_dyeh::{DyehLogProvider, DyehParser};
use lazylog_framework::start_with_provider;
use lazylog_ios::{IosEffectParser, IosFullParser, IosLogProvider};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        style::{Color, ResetColor, SetBackgroundColor},
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
use std::time::Duration;

fn print_usage() {
    eprintln!("Usage: lazylog [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --ios, -i               Use iOS full parser");
    eprintln!("  --ios-effect, -ie       Use iOS effect parser");
    eprintln!("  --android, -a           Use Android adb logcat provider");
    eprintln!("  --android-effect, -ae   Use Android effect parser");
    eprintln!("  --dyeh, -dy             Use DYEH file-based log provider (default)");
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

enum UsageOptions {
    IosEffect,
    IosFull,
    Android,
    AndroidEffect,
    Dyeh,
    Help,
    None, // default when no args provided
}

impl UsageOptions {
    fn from_args(args: &[String]) -> Result<Self, io::Error> {
        match args.len() {
            0 => Ok(Self::None),
            1 => match args[0].as_str() {
                "--ios-effect" | "-ie" => Ok(Self::IosEffect),
                "--ios" | "-i" => Ok(Self::IosFull),
                "--android" | "-a" => Ok(Self::Android),
                "--android-effect" | "-ae" => Ok(Self::AndroidEffect),
                "--dyeh" | "-dy" => Ok(Self::Dyeh),
                "--help" | "-h" => Ok(Self::Help),
                _ => {
                    print_usage();
                    Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Unknown option",
                    ))
                }
            },
            _ => {
                print_usage();
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Only zero or one argument is allowed",
                ))
            }
        }
    }
}

fn main() -> io::Result<()> {
    // Collect args excluding the binary name
    let args: Vec<String> = env::args().skip(1).collect();
    let usage_option = UsageOptions::from_args(&args)?;

    if let UsageOptions::Help = usage_option {
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

    // Prepare provider and parser based on option (default to DYEH)
    let app_result = match usage_option {
        UsageOptions::IosEffect => {
            let provider = IosLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(IosEffectParser::new());
            start_with_provider(&mut terminal, provider, parser)
        }
        UsageOptions::IosFull => {
            let provider = IosLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(IosFullParser::new());
            start_with_provider(&mut terminal, provider, parser)
        }
        UsageOptions::Android => {
            let provider = AndroidLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(AndroidParser::new());
            start_with_provider(&mut terminal, provider, parser)
        }
        UsageOptions::AndroidEffect => {
            let provider = AndroidLogProvider::new();
            let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                Arc::new(AndroidEffectParser::new());
            start_with_provider(&mut terminal, provider, parser)
        }
        UsageOptions::Dyeh | UsageOptions::None => {
            if let Some(dir) = dirs::home_dir() {
                let log_dir_path = dir.join("Library/Application Support/DouyinAR");
                let provider = DyehLogProvider::new(log_dir_path);
                let parser: Arc<dyn lazylog_framework::provider::LogParser> =
                    Arc::new(DyehParser::new());
                start_with_provider(&mut terminal, provider, parser)
            } else {
                eprintln!("Error: Could not determine home directory");
                Ok(())
            }
        }
        UsageOptions::Help => unreachable!(),
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
