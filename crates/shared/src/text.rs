//! Text normalization for text-to-speech.
//!
//! LLMs answer with Markdown, code blocks and emojis. Kokoro expects plain
//! prose: those artifacts are removed here so every TTS backend receives the
//! same clean input.

/// Returns a plain-prose version of `input`, suitable for speech synthesis.
///
/// The transformation is deliberately conservative: it keeps punctuation and
/// accents (they carry prosody) and only removes markup, code, links syntax,
/// list markers and emojis.
pub fn for_speech(input: &str) -> String {
    let without_fences = strip_code_fences(input);
    let without_links = strip_links(&without_fences);

    let mut cleaned = String::with_capacity(without_links.len());
    for ch in without_links.chars() {
        if is_emoji(ch) {
            continue;
        }
        if matches!(ch, '`' | '*' | '_' | '#' | '>' | '|' | '~') {
            continue;
        }
        cleaned.push(ch);
    }

    let mut lines = Vec::new();
    for line in cleaned.lines() {
        let line = strip_leading_marker(line);
        let line = line.trim();
        if !line.is_empty() {
            lines.push(line);
        }
    }

    let joined = lines.join(" ");
    collapse_whitespace(&joined)
}

fn strip_code_fences(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_fence = false;

    for line in input.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            out.push_str(line);
            out.push('\n');
        }
    }

    out
}

fn strip_links(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut index = 0;

    while index < input.len() {
        let rest = &input[index..];
        if rest.starts_with('[')
            && let Some(text_end_rel) = rest.find("](")
        {
            let text_end = index + text_end_rel;
            if let Some(paren_rel) = input[text_end + 2..].find(')') {
                out.push_str(&input[index + 1..text_end]);
                index = text_end + 2 + paren_rel + 1;
                continue;
            }
        }

        let ch = rest.chars().next().expect("index is on a char boundary");
        out.push(ch);
        index += ch.len_utf8();
    }

    out
}

fn strip_leading_marker(line: &str) -> &str {
    let trimmed = line.trim_start();

    if let Some(rest) = trimmed.strip_prefix("- ") {
        return rest;
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        return rest;
    }
    if let Some(rest) = trimmed.strip_prefix("+ ") {
        return rest;
    }
    if let Some(rest) = trimmed.strip_prefix('•') {
        return rest.trim_start();
    }

    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    if !digits.is_empty()
        && let Some(rest) = trimmed[digits.len()..].strip_prefix(". ")
    {
        return rest;
    }

    trimmed.trim_start_matches('#').trim_start()
}

fn is_emoji(ch: char) -> bool {
    matches!(
        ch,
        '\u{1F000}'..='\u{1FAFF}'
            | '\u{2600}'..='\u{27BF}'
            | '\u{2B00}'..='\u{2BFF}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{200D}'
            | '\u{20E3}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;

    for ch in input.chars() {
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        out.push(ch);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_code_fences_but_keeps_prose() {
        let input = "Aquí tienes:\n```rust\nfn main() {}\n```\nEspero que ayude.";
        assert_eq!(for_speech(input), "Aquí tienes: Espero que ayude.");
    }

    #[test]
    fn keeps_inline_code_content_without_backticks() {
        assert_eq!(
            for_speech("Usa `llama-server --port 9931` para empezar."),
            "Usa llama-server --port 9931 para empezar."
        );
    }

    #[test]
    fn converts_links_to_their_text() {
        assert_eq!(
            for_speech("Mira [la documentación](https://example.com) ahora."),
            "Mira la documentación ahora."
        );
    }

    #[test]
    fn strips_list_markers_and_headers() {
        let input = "## Pasos\n1. Abre la app\n- Pulsa iniciar\n* Listo";
        assert_eq!(for_speech(input), "Pasos Abre la app Pulsa iniciar Listo");
    }

    #[test]
    fn removes_emojis_and_zwj_sequences() {
        assert_eq!(for_speech("Hola 👋🏽 mundo 🚀!"), "Hola mundo !");
        assert_eq!(for_speech("Familia 👨‍👩‍👧 lista"), "Familia lista");
    }

    #[test]
    fn preserves_spanish_accents_and_punctuation() {
        assert_eq!(
            for_speech("¿Qué tal? ¡Muy bien! Añadí código, ¿eh?"),
            "¿Qué tal? ¡Muy bien! Añadí código, ¿eh?"
        );
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(for_speech("  uno \n\n dos\t tres  "), "uno dos tres");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(for_speech("   \n\n "), "");
    }
}
