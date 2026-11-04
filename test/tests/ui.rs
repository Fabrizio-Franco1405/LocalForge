//! Tests for `ui` models: page labels, chat log state machine and colors.
//!
//! They do not open a window: `ChatLog` and `Page` are pure data.

use ui::{SettingsTab, WorkspaceMode};
use ui::chat::ChatLog;
use ui::theme::parse_hex;

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
