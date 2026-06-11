use std::io;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Creates backfround log to /tmp/stic.log that won't corrupt TUI
    if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/stic.log") {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
            .target(env_logger::Target::Pipe(Box::new(file)))
            .try_init()
            .ok();
    }

    let args: Vec<String> = std::env::args().skip(1).collect();

    // Transitions the standard terminal window into a dedicated terminal application window
    // https://docs.rs/crossterm/0.29.0/crossterm/terminal/index.html#raw-mode
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize app state
    let mut app = app::App::new();

    for path in &args {
        if let Err(e) = app.open_file(std::path::Path::new(path)) {
            app.set_message(format!("Error opening {path}: {e}"));
        }
    }

    // Main application loop
    while !app.should_quit {
        // Let ui::render method draw the ui
        terminal.draw(|f| ui::render(f, &mut app))?;
        // Process input commands
        input::handle_input(&mut app)?;
        // Updated the screen
        app.tick();
    }

    // Reset terminal default settings
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
