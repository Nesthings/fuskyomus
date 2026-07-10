mod app;
mod art;
mod config;
mod library;
mod player;
mod ui;

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{self, Event};

use app::App;

fn main() -> anyhow::Result<()> {
    let library_roots = resolve_library_roots();

    silence_stderr_to_log();

    let (event_tx, event_rx) = mpsc::channel();
    let cmd_tx = player::spawn(event_tx);
    let mut app = App::new(library_roots, cmd_tx.clone());

    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, &mut app, &event_rx);
    ratatui::restore();

    let _ = cmd_tx.send(player::PlayerCommand::Shutdown);
    result
}

/// Decides which directories to browse on startup, in priority order:
/// 1. An explicit CLI argument -- a one-off override, doesn't touch the
///    saved config at all.
/// 2. The directories saved via the in-app Settings view.
/// 3. A best-effort guess at the system's music folder, falling back to
///    `$HOME` and finally the current directory.
fn resolve_library_roots() -> Vec<PathBuf> {
    if let Some(arg) = std::env::args().nth(1) {
        let path = PathBuf::from(arg);
        if path.is_dir() {
            return vec![path];
        }
        eprintln!(
            "'{}' is not a directory. Usage: fuskyom [music-directory]",
            path.display()
        );
        std::process::exit(1);
    }

    let saved: Vec<PathBuf> = config::load_library_paths()
        .into_iter()
        .filter(|p| p.is_dir())
        .collect();
    if !saved.is_empty() {
        return saved;
    }

    let fallback = dirs::audio_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    vec![fallback]
}

/// ALSA (and occasionally other native audio libs) write warnings like
/// "underrun occurred" straight to stderr, bypassing our TUI entirely and
/// corrupting the screen. Those messages are harmless noise from the audio
/// backend, not errors from this app, so we redirect stderr to a log file
/// instead of letting them splatter over the alternate screen. Anything
/// genuinely worth showing to the user goes through PlayerEvent::Error and
/// is rendered in the status line instead.
fn silence_stderr_to_log() {
    let log_path = std::env::temp_dir().join("fuskyom-stderr.log");
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    else {
        return;
    };
    redirect_stderr_to(file);
}

#[cfg(unix)]
fn redirect_stderr_to(file: std::fs::File) {
    use std::os::unix::io::AsRawFd;
    unsafe {
        libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO);
    }
    // Leak the handle on purpose: fd 2 now points at it for the whole
    // process lifetime, closing our end here would be wrong.
    std::mem::forget(file);
}

#[cfg(windows)]
fn redirect_stderr_to(file: std::fs::File) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE};

    unsafe {
        SetStdHandle(STD_ERROR_HANDLE, file.as_raw_handle() as _);
    }
    // Leak the handle on purpose, same reasoning as the Unix side: the OS
    // now owns this as the process's stderr for its whole lifetime.
    std::mem::forget(file);
}

#[cfg(not(any(unix, windows)))]
fn redirect_stderr_to(_file: std::fs::File) {
    // Unknown platform: leave stderr alone rather than guessing at an API.
}

fn run_app(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    event_rx: &mpsc::Receiver<player::PlayerEvent>,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        // Drain any pending events from the audio thread first so the UI
        // reflects the latest playback state before we redraw.
        while let Ok(event) = event_rx.try_recv() {
            app.on_player_event(event);
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    app.handle_key(key);
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
