//! Tool discovery and path helpers.
//!
//! LocalForge does not install anything: it resolves what the user configured,
//! reports whether it exists, and lets the UI explain what is missing.

use std::path::{Path, PathBuf};

/// Expands `~` and returns `None` for empty input.
pub fn expand_path(input: &str) -> Option<PathBuf> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    if let Some(rest) = input.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return Some(home.join(rest));
    }

    Some(PathBuf::from(input))
}

/// Resolves a configured binary.
///
/// - A path with separators is checked as-is.
/// - A bare name is searched in `PATH` (and `PATHEXT` on Windows).
pub fn resolve_binary(binary: &Path) -> Option<PathBuf> {
    let has_separator = binary.components().count() > 1 || binary.is_absolute();
    if has_separator {
        return binary.is_file().then(|| binary.to_path_buf());
    }

    let name = binary.to_str()?;
    let path = std::env::var_os("PATH")?;

    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }

        #[cfg(windows)]
        {
            let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT".to_owned());
            for extension in pathext.split(';') {
                let extension = extension.trim_start_matches('.');
                if extension.is_empty() {
                    continue;
                }
                let candidate = directory.join(format!("{name}.{extension}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

/// Returns a displayable status for a file path.
pub fn file_status(path: Option<&Path>) -> (bool, String) {
    match path {
        None => (false, "sin configurar".to_owned()),
        Some(path) if path.as_os_str().is_empty() => (false, "sin configurar".to_owned()),
        Some(path) if path.is_file() => (true, path.display().to_string()),
        Some(path) => (false, format!("no existe: {}", path.display())),
    }
}

/// First token of a command template, used only for diagnostics.
pub fn command_program(command: &str) -> Option<&str> {
    command.split_whitespace().next()
}
