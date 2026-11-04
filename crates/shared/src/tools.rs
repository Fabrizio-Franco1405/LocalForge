//! Tool discovery and path helpers.
//!
//! VoxLocal does not install anything: it resolves what the user configured,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_path_expands_to_none() {
        assert_eq!(expand_path("   "), None);
    }

    #[test]
    fn absolute_path_is_preserved() {
        assert_eq!(
            expand_path("/tmp/model.gguf"),
            Some(PathBuf::from("/tmp/model.gguf"))
        );
    }

    #[test]
    fn tilde_is_expanded() {
        let expanded = expand_path("~/model.gguf").expect("expanded");
        assert!(expanded.is_absolute());
        assert!(expanded.ends_with("model.gguf"));
        assert!(!expanded.to_string_lossy().contains('~'));
    }

    #[test]
    fn missing_absolute_binary_is_reported() {
        assert!(resolve_binary(Path::new("/definitely/not/a/binary")).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn shell_is_found_in_path() {
        assert!(resolve_binary(Path::new("sh")).is_some());
    }

    #[test]
    fn file_status_distinguishes_missing() {
        let (ok, value) = file_status(Some(Path::new("/definitely/not/here")));
        assert!(!ok);
        assert!(value.contains("no existe"));
    }

    #[test]
    fn command_program_reads_first_token() {
        assert_eq!(
            command_program("piper --model voice.onnx --output_file {out}"),
            Some("piper")
        );
        assert_eq!(command_program("   "), None);
    }
}
