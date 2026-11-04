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

/// Shortens `input` to at most `max` characters, appending `…` when cut.
///
/// Counts `char`s (not bytes), so the result is always valid UTF-8 and never
/// splits a multi-byte character. Used for diagnostics and error details.
pub fn truncate_chars(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_owned();
    }

    let mut truncated: String = input.chars().take(max).collect();
    truncated.push('…');
    truncated
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
