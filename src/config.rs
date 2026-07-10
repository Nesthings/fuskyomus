use std::io::Write;
use std::path::PathBuf;

/// Where the list of default library directories lives on disk, e.g.
/// `~/.config/fuskyom/library_paths.txt` on Linux (via `dirs::config_dir()`,
/// which respects `$XDG_CONFIG_HOME`).
fn config_file() -> Option<PathBuf> {
    let mut dir = dirs::config_dir()?;
    dir.push("fuskyom");
    Some(dir.join("library_paths.txt"))
}

/// Reads the saved library root directories: one path per non-empty,
/// non-comment (`#`) line. A leading `~` is expanded to the home directory.
/// Returns an empty list if the file doesn't exist yet or can't be read --
/// that's a normal "nothing configured yet" state, not an error worth
/// surfacing.
pub fn load_library_paths() -> Vec<PathBuf> {
    let Some(path) = config_file() else {
        return Vec::new();
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    contents
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(expand_tilde)
        .collect()
}

/// Overwrites the saved list of library root directories on disk, creating
/// the config directory if needed.
pub fn save_library_paths(paths: &[PathBuf]) -> std::io::Result<()> {
    let path = config_file().ok_or_else(|| {
        std::io::Error::other("could not determine the config directory for this platform")
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(&path)?;
    writeln!(file, "# fuskyom default music directories, one per line.")?;
    for p in paths {
        writeln!(file, "{}", p.display())?;
    }
    Ok(())
}

/// Expands a leading `~` or `~/...` to the user's home directory. Anything
/// else is returned as-is (relative paths are left relative to whatever the
/// working directory happens to be, which is fine since we validate with
/// `.is_dir()` right after loading).
pub fn expand_tilde(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}

pub fn load_theme() -> usize {
    let Some(mut path) = config_file() else {
        return 0;
    };
    path.pop();
    path.push("theme.txt");
    std::fs::read_to_string(&path)
        .unwrap_or_default()
        .trim()
        .parse()
        .unwrap_or(0)
}

pub fn save_theme(idx: usize) -> std::io::Result<()> {
    let Some(mut path) = config_file() else {
        return Ok(());
    };
    path.pop();
    path.push("theme.txt");
    let mut file = std::fs::File::create(path)?;
    write!(file, "{idx}")?;
    Ok(())
}

pub fn load_visualizer() -> bool {
    let Some(mut path) = config_file() else {
        return true;
    };
    path.pop();
    path.push("visualizer.txt");
    std::fs::read_to_string(&path)
        .unwrap_or_default()
        .trim()
        .parse()
        .unwrap_or(true)
}

pub fn save_visualizer(show: bool) -> std::io::Result<()> {
    let Some(mut path) = config_file() else {
        return Ok(());
    };
    path.pop();
    path.push("visualizer.txt");
    let mut file = std::fs::File::create(path)?;
    write!(file, "{show}")?;
    Ok(())
}

pub fn load_vis_style() -> usize {
    let Some(mut path) = config_file() else {
        return 0;
    };
    path.pop();
    path.push("vis_style.txt");
    std::fs::read_to_string(&path)
        .unwrap_or_default()
        .trim()
        .parse()
        .unwrap_or(0)
}

pub fn save_vis_style(style: usize) -> std::io::Result<()> {
    let Some(mut path) = config_file() else {
        return Ok(());
    };
    path.pop();
    path.push("vis_style.txt");
    let mut file = std::fs::File::create(path)?;
    write!(file, "{style}")?;
    Ok(())
}
