mod app;
mod app_block;
mod content_line_maker;
mod file_finder;
mod log_list;
mod log_parser;
mod metadata;
mod theme;
mod ui_logger;

use crossterm::event;
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

    let app_result = app::start(&mut terminal);

    restore_terminal()?;

    if let Err(err) = app_result {
        println!("Application Error: {:?}", err);
    }

    Ok(())
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    // Enable raw mode to process key events without OS interference.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // Enter the alternate screen to not mess with the user's shell history.
    // Enable mouse capture to receive mouse events.
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal() -> io::Result<()> {
    let mut stdout = io::stdout();

    // stop the terminal from sending further mouse packets
    execute!(stdout, DisableMouseCapture)?;

    // switch back to the normal screen (still in raw mode, no echo)
    execute!(stdout, LeaveAlternateScreen)?;

    // drain any pending events so nothing is left in the input buffer.
    while event::poll(Duration::from_millis(0))? {
        let _ = event::read()?;
    }

    // finally leave raw mode (echo/canonical back on)
    disable_raw_mode()?;

    Ok(())
}
