use crossterm::event;
use lazylog_dyeh::DyehLogProvider;
use lazylog_framework::start_with_provider;
use lazylog_ios::IosLogProvider;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
};
use std::env;
use std::io;
use std::panic;
use std::time::Duration;

fn print_usage() {
    eprintln!("Usage: lazylog [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --ios        Use iOS device log provider (via syslog relay)");
    eprintln!("  --dyeh       Use DYEH file-based log provider (default)");
    eprintln!("  --help       Print this help message");
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    // parse command-line arguments
    let use_ios = args.iter().any(|arg| arg == "--ios");
    let _use_dyeh = args.iter().any(|arg| arg == "--dyeh");
    let show_help = args.iter().any(|arg| arg == "--help" || arg == "-h");

    if show_help {
        print_usage();
        return Ok(());
    }

    let mut terminal = setup_terminal()?;

    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        restore_terminal().unwrap();
        original_hook(panic_info);
    }));

    let app_result = if use_ios {
        // iOS provider
        let provider = IosLogProvider::new();
        start_with_provider(&mut terminal, provider)
    } else {
        // DYEH provider (default)
        let log_dir_path = match dirs::home_dir() {
            Some(path) => path.join("Library/Application Support/DouyinAR"),
            None => {
                eprintln!("Error: Could not determine home directory");
                restore_terminal()?;
                return Ok(());
            }
        };

        let provider = DyehLogProvider::new(log_dir_path);
        start_with_provider(&mut terminal, provider)
    };

    restore_terminal()?;

    if let Err(err) = app_result {
        println!("Application Error: {:?}", err);
    }

    Ok(())
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // enter the alternate screen to not mess with the user's shell history
    // enable mouse capture to receive mouse events
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal() -> io::Result<()> {
    let mut stdout = io::stdout();

    execute!(stdout, DisableMouseCapture)?;

    execute!(stdout, LeaveAlternateScreen)?;

    while event::poll(Duration::from_millis(0))? {
        let _ = event::read()?;
    }

    disable_raw_mode()?;

    Ok(())
}
