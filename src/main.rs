use std::io;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

fn main() -> Result<()> {
    // Log to /tmp/stic.log — won't corrupt TUI
    if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/stic.log") {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
            .target(env_logger::Target::Pipe(Box::new(file)))
            .try_init()
            .ok();
    }

    let args: Vec<String> = std::env::args().skip(1).collect();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = app::App::new();

    for path in &args {
        if let Err(e) = app.open_file(std::path::Path::new(path)) {
            app.set_message(format!("Error opening {path}: {e}"));
        }
    }

    while !app.should_quit {
        terminal.draw(|f| ui::render(f, &mut app))?;
        input::handle_input(&mut app)?;
        app.tick();
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
