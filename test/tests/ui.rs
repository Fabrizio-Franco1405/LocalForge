//! Tests for `ui` models: page labels, chat log state machine and colors.
//!
//! They do not open a window: `ChatLog` and `Page` are pure data.

use ui::chat::ChatLog;
use ui::theme::parse_hex;
use ui::{SettingsTab, WorkspaceMode};
#[test]
fn workspace_labels_are_stable() {
    assert_eq!(WorkspaceMode::Chat.label(), "Chat");
    assert_eq!(WorkspaceMode::Call.label(), "Llamada");
    assert_eq!(WorkspaceMode::Studio3d.label(), "Estudio 3D");
    assert_eq!(WorkspaceMode::Settings.label(), "Ajustes");
}

#[test]
fn settings_tab_labels_are_stable() {
    assert_eq!(SettingsTab::Llm.label(), "Motor LLM");
    assert_eq!(SettingsTab::Audio.label(), "Motor Audio");
    assert_eq!(SettingsTab::Scene3d.label(), "Motor 3D");
    assert_eq!(SettingsTab::General.label(), "General");
    assert_eq!(SettingsTab::System.label(), "Sistema");
    assert_eq!(SettingsTab::Mcp.label(), "MCP");
}

mod chat_log {
    use super::*;

    #[test]
    fn streamed_tokens_build_one_bubble() {
        let mut log = ChatLog::new();
        log.push_user("Hola");
        log.begin_reply();

        assert!(log.append_delta("Bue"));
        assert!(log.append_delta("nas"));
        assert_eq!(log.len(), 2);
        assert_eq!(log.bubbles()[1].text, "Buenas");
        assert!(!log.bubbles()[1].complete);

        log.finish_reply("Buenas");
        assert!(log.bubbles()[1].complete);
    }

    #[test]
    fn deltas_after_finish_are_ignored() {
        let mut log = ChatLog::new();
        log.begin_reply();
        log.finish_reply("Listo");

        assert!(!log.append_delta("tarde"));
        assert_eq!(log.last_text(), Some("Listo"));
    }

    #[test]
    fn cancelling_keeps_partial_text() {
        let mut log = ChatLog::new();
        log.begin_reply();
        log.append_delta("parcial");
        log.finish_reply("");

        assert_eq!(log.last_text(), Some("parcial"));
    }

    #[test]
    fn empty_pending_reply_is_dropped() {
        let mut log = ChatLog::new();
        log.begin_reply();
        log.drop_empty_reply();

        assert!(log.is_empty());
    }

    #[test]
    fn deltas_without_open_reply_are_rejected() {
        let mut log = ChatLog::new();

        assert!(!log.append_delta("huérfano"));
        log.push_user("Hola");
        assert!(!log.append_delta("huérfano"));
    }
}

#[test]
fn parses_short_and_long_hex_colors() {
    assert!(parse_hex("#7c5cff").is_some());
    assert!(parse_hex("#fff").is_some());
    assert!(parse_hex("not-a-color").is_none());
}

/// Selectores de archivos: opciones del prompt nativo de GPUI y regresión del
/// cierre de Windows al abrir un diálogo modal bloqueante (`rfd`).
mod native_dialogs {
    use std::path::Path;

    use ui::dialog::Pick;

    #[test]
    fn folder_picker_accepts_only_directories() {
        let options = Pick::Folder.options();

        assert!(options.directories);
        assert!(!options.files);
        assert!(!options.multiple);
    }

    #[test]
    fn file_picker_accepts_files_and_attachment_picker_allows_many() {
        let single = Pick::File.options();
        assert!(single.files);
        assert!(!single.directories);
        assert!(!single.multiple);

        let many = Pick::Files.options();
        assert!(many.files);
        assert!(!many.directories);
        assert!(many.multiple);
    }

    #[test]
    fn picker_button_labels_are_stable() {
        assert_eq!(Pick::File.button_label(), "Elegir archivo…");
        assert_eq!(Pick::Files.button_label(), "Elegir archivos…");
        assert_eq!(Pick::Folder.button_label(), "Elegir carpeta…");
    }

    #[test]
    fn ui_never_uses_blocking_rfd_dialogs() {
        let manifest = std::fs::read_to_string("../crates/ui/Cargo.toml")
            .expect("no se pudo leer crates/ui/Cargo.toml");

        assert!(
            !manifest.contains("rfd"),
            "ui/Cargo.toml volvió a depender de rfd: su diálogo modal reentra en GPUI sobre \
             Windows y mata el proceso (RefCell already borrowed, 0xc0000409). Usa \
             cx.prompt_for_paths a través de ui::dialog."
        );

        let sources = rust_sources(Path::new("../crates/ui/src"));
        assert!(
            !sources.is_empty(),
            "no se encontraron fuentes en crates/ui/src"
        );

        for (path, source) in sources {
            for line in source.lines() {
                let code = line.trim_start();
                // Los comentarios pueden nombrar el crate prohibido para explicar el fallo.
                if code.starts_with("//") {
                    continue;
                }

                assert!(
                    !code.contains("rfd"),
                    "{path} usa rfd: el diálogo bloqueante reentra en GPUI sobre Windows y mata \
                     la app. Usa ui::dialog."
                );
            }
        }
    }

    fn rust_sources(dir: &Path) -> Vec<(String, String)> {
        let mut files = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return files;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(rust_sources(&path));
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                files.push((path.display().to_string(), source));
            }
        }

        files
    }
}
