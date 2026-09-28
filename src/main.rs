use color_eyre::Result;
use crossterm::{
    cursor::Show,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

use sqwatch::dashboard::Dashboard;

const USAGE: &str = concat!(
    env!("CARGO_PKG_DESCRIPTION"),
    "

Usage: sqwatch [OPTIONS]

Options:
  -h, --help     Print help
  -V, --version  Print version
"
);

/// Restore the terminal to its normal state. Safe to call more than once.
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        Show
    );
}

/// Restores the terminal when dropped, so a normal return, an error, or a
/// panic all leave the shell usable instead of stuck in raw mode.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn main() -> Result<()> {
    // Answer or reject any argument before the terminal is touched, so
    // `--help` and `--version` print a string instead of opening the UI.
    if let Some(arg) = std::env::args_os().nth(1) {
        match arg.to_str() {
            Some("-h" | "--help") => print!("{}", USAGE),
            Some("-V" | "--version") => println!("sqwatch {}", env!("CARGO_PKG_VERSION")),
            _ => {
                eprint!(
                    "sqwatch: unrecognized argument '{}'\n\n{}",
                    arg.to_string_lossy(),
                    USAGE
                );
                std::process::exit(2);
            }
        }
        return Ok(());
    }

    color_eyre::install()?;

    // Chain a terminal restore in front of color_eyre's panic hook so a panic
    // never leaves the user in the alternate screen with raw mode enabled.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous_hook(info);
    }));

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let _guard = TerminalGuard;

    let term_backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(term_backend)?;

    let outcome = Dashboard::new().and_then(|mut app| app.run(&mut terminal));

    // Restore before printing any error so the report is readable.
    drop(_guard);

    if let Err(err) = outcome {
        eprintln!("{}", err);
        std::process::exit(1);
    }

    Ok(())
}
