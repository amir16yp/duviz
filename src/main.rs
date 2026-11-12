mod scan;
mod treemap;
mod ui;

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use scan::Entry;
use ui::{draw, App};

#[derive(Parser, Debug)]
#[command(
    name = "duviz",
    about = "WizTree-style disk usage visualizer for the terminal",
    version
)]
struct Cli {
    /// Directory to scan (defaults to current working directory)
    #[arg(value_name = "DIR")]
    directory: Option<PathBuf>,

    /// Disable colored output
    #[arg(long = "nocolors")]
    no_colors: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root_path = match cli.directory {
        Some(p) => p,
        None => std::env::current_dir().context("failed to get current directory")?,
    };

    if !root_path.exists() {
        bail!("path does not exist: {}", root_path.display());
    }
    if !root_path.is_dir() {
        bail!("not a directory: {}", root_path.display());
    }

    let root_display = root_path.display().to_string();
    let started = Instant::now();
    let mut last_draw = Instant::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .unwrap_or_else(Instant::now);

    let (tree, stats) = Entry::scan_with_progress(&root_path, |stats| {
        // Throttle redraws so huge trees don't spend all time printing.
        if last_draw.elapsed() < std::time::Duration::from_millis(50) {
            return;
        }
        last_draw = Instant::now();
        eprint!(
            "\r\x1b[2KScanning {root_display} … {} files · {} dirs · {} · {:.1}s",
            stats.files,
            stats.dirs,
            scan::human_size(stats.bytes),
            started.elapsed().as_secs_f64(),
        );
        let _ = io::stderr().flush();
    })
    .with_context(|| format!("failed to scan {}", root_path.display()))?;

    eprintln!(
        "\r\x1b[2KScanning {root_display} … {} files · {} dirs · {} · done in {:.2}s",
        stats.files,
        stats.dirs,
        scan::human_size(stats.bytes),
        started.elapsed().as_secs_f64(),
    );

    let use_colors = !cli.no_colors;
    let mut app = App::new(tree, use_colors);
    run_tui(&mut app)?;
    Ok(())
}

fn run_tui(app: &mut App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;

        if !event::poll(std::time::Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => break,
            KeyCode::Tab => app.toggle_focus(),
            KeyCode::Down | KeyCode::Char('j') => app.select_next(),
            KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
            KeyCode::PageDown => app.page_down(),
            KeyCode::PageUp => app.page_up(),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => app.enter(),
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => app.leave(),
            KeyCode::Home => app.select_home(),
            KeyCode::End => app.select_end(),
            _ => {}
        }
    }
    Ok(())
}
