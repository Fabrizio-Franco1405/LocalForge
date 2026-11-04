//! Conversation model and bubble rendering.

use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Who wrote a bubble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    /// The person using the app.
    User,
    /// The connected language model.
    Assistant,
}

impl Speaker {
    /// Label shown above the bubble.
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "Tú",
            Self::Assistant => "Asistente",
        }
    }
}

/// One line of the conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bubble {
    /// Author.
    pub speaker: Speaker,
    /// Text produced so far.
    pub text: String,
    /// `false` while tokens are still arriving.
    pub complete: bool,
}

/// Ordered conversation. Pure data so it can be tested without a window.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChatLog {
    bubbles: Vec<Bubble>,
}

impl ChatLog {
    /// Empty conversation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether no bubble has been added yet.
    pub fn is_empty(&self) -> bool {
        self.bubbles.is_empty()
    }

    /// All bubbles, oldest first.
    pub fn bubbles(&self) -> &[Bubble] {
        &self.bubbles
    }

    /// Number of bubbles.
    pub fn len(&self) -> usize {
        self.bubbles.len()
    }

    /// Appends a finished user message.
    pub fn push_user(&mut self, text: impl Into<String>) {
        self.bubbles.push(Bubble {
            speaker: Speaker::User,
            text: text.into(),
            complete: true,
        });
    }

    /// Adds the empty assistant bubble that receives streamed tokens.
    pub fn begin_reply(&mut self) {
        self.bubbles.push(Bubble {
            speaker: Speaker::Assistant,
            text: String::new(),
            complete: false,
        });
    }

    /// Appends a streamed token to the open assistant bubble.
    ///
    /// Returns `false` when there is no open bubble (a stale generation).
    pub fn append_delta(&mut self, delta: &str) -> bool {
        match self.bubbles.last_mut() {
            Some(bubble) if !bubble.complete && bubble.speaker == Speaker::Assistant => {
                bubble.text.push_str(delta);
                true
            }
            _ => false,
        }
    }

    /// Closes the open assistant bubble with the final text.
    ///
    /// When `text` is empty the partial stream is kept, so cancelling does not
    /// erase what the user already saw.
    pub fn finish_reply(&mut self, text: &str) {
        if let Some(bubble) = self.bubbles.last_mut()
            && bubble.speaker == Speaker::Assistant
            && !bubble.complete
        {
            if !text.is_empty() {
                bubble.text = text.to_owned();
            }
            bubble.complete = true;
        }
    }

    /// Removes the trailing empty assistant bubble, if any.
    pub fn drop_empty_reply(&mut self) {
        if let Some(bubble) = self.bubbles.last()
            && bubble.speaker == Speaker::Assistant
            && !bubble.complete
            && bubble.text.trim().is_empty()
        {
            self.bubbles.pop();
        }
    }

    /// Clears the conversation.
    pub fn clear(&mut self) {
        self.bubbles.clear();
    }

    /// Text of the last bubble, used for "copy last reply" affordances.
    pub fn last_text(&self) -> Option<&str> {
        self.bubbles.last().map(|bubble| bubble.text.as_str())
    }
}

/// Renders one bubble.
pub fn render_bubble(bubble: &Bubble, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let (background, foreground, border) = match bubble.speaker {
        Speaker::User => (theme.primary, theme.primary_foreground, theme.primary),
        Speaker::Assistant => (theme.secondary, theme.secondary_foreground, theme.border),
    };

    let is_empty = bubble.text.is_empty();

    div()
        .flex()
        .flex_col()
        .gap_1()
        .max_w(px(720.))
        .w_full()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(theme.muted_foreground)
                .child(bubble.speaker.label()),
        )
        .child(
            div()
                .w_full()
                .rounded_md()
                .border_1()
                .border_color(border)
                .bg(background)
                .text_color(foreground)
                .px_3()
                .py_2()
                .text_sm()
                .whitespace_normal()
                .when(is_empty, |this| {
                    this.child(div().text_color(theme.muted_foreground).child("Pensando…"))
                })
                .when(!is_empty, |this| this.child(bubble.text.clone())),
        )
}

#[cfg(test)]
mod tests {
    use super::ChatLog;

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
