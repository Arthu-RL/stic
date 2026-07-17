use std::io;
use std::panic;
use std::path::Path;

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

    panic::set_hook(Box::new(|info: &panic::PanicHookInfo<'_>| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::event::DisableMouseCapture);
        let _ = execute!(io::stdout(), crossterm::cursor::Show);
        eprintln!("Application panicked critical error: {info}");
    }));

    // Transitions the standard terminal window into a dedicated terminal application window
    // https://docs.rs/crossterm/0.29.0/crossterm/terminal/index.html#raw-mode
    enable_raw_mode()?;
    let mut stdout: io::Stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend: CrosstermBackend<io::Stdout> = CrosstermBackend::new(stdout);
    let mut terminal: Terminal<CrosstermBackend<io::Stdout>> = Terminal::new(backend)?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let result: std::prelude::v1::Result<(), anyhow::Error> = run_app(&mut terminal, args).await;

    // Terminal Reset
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Propagate whatever result came out of the app execution
    result
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, args: Vec<String>) -> Result<()> {
    let mut app: app::App = app::App::new();

    for path in &args {
        if let Err(e) = app.open_file(Path::new(path)) {
            app.set_message(format!("Error opening {path}: {e}"));
        }
    }

    while !app.should_quit {
        terminal.draw(|f: &mut Frame<'_>| ui::Ui::render(f, &mut app))?;
        input::handle_input(&mut app)?;
        app.tick();
    }

    // Gracefully tear down every language server (LSP `shutdown` request +
    // `exit` notification) before the process ends, so servers such as
    // rust-analyzer terminate cleanly instead of panicking about a client
    // that "exited without proper shutdown sequence".
    if let Some(lsp) = app.lsp.take() {
        lsp.shutdown_all().await;
    }

    Ok(())
}