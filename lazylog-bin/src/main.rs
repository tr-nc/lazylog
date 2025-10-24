use crossterm::event;
use lazylog_dyeh::DyehLogProvider;
use lazylog_framework::start_with_provider;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
};
use std::io;
use std::panic;
use std::time::Duration;

fn main() -> io::Result<()> {
    let mut terminal = setup_terminal()?;

    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        restore_terminal().unwrap();
        original_hook(panic_info);
    }));

    // DYEH-specific: hardcoded path to DouyinAR logs
    let log_dir_path = match dirs::home_dir() {
        Some(path) => path.join("Library/Application Support/DouyinAR"),
        None => {
            eprintln!("Error: Could not determine home directory");
            return Ok(());
        }
    };

    let provider = DyehLogProvider::new(log_dir_path);
    let app_result = start_with_provider(&mut terminal, provider);

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
