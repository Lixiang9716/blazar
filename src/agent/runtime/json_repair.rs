use log::warn;
use serde_json::Value;

pub(super) struct ParsedToolArgs {
    pub(super) value: Value,
    pub(super) was_repaired: bool,
}

/// Strip `<think>...</think>` reasoning blocks that some models (e.g. Qwen3)
/// may embed in tool call arguments. Falls back to the original string if no
/// tags are found. Also attempts to extract a JSON substring if the result
/// still doesn't start with `{` or `[`.
pub(super) fn strip_thinking_tags(raw: &str) -> String {
    let mut s = raw.to_string();

    // Remove <think>...</think> blocks (greedy across lines).
    if let Some(start) = s.find("<think>")
        && let Some(end) = s.find("</think>")
    {
        let tag_end = end + "</think>".len();
        s = format!("{}{}", &s[..start], s[tag_end..].trim_start());
    }

    // If the result still doesn't look like JSON, try to find the first `{`.
    let trimmed = s.trim();
    if !trimmed.starts_with('{')
        && !trimmed.starts_with('[')
        && let Some(idx) = trimmed.find('{')
    {
        return trimmed[idx..].to_string();
    }

    s
}

/// Try standard JSON parse first. On failure, apply minimal targeted
/// repairs for well-understood malformations (tag stripping, control chars).
///
/// Industry pattern (from Codex CLI, Continue.dev, Aider research):
/// complex hand-rolled repair for arbitrary JSON is a losing game.
/// Keep only simple, correct repairs. For anything else, return the
/// error so the model gets feedback and can retry with valid JSON.
pub(super) fn parse_or_repair_json(raw: &str) -> Result<ParsedToolArgs, serde_json::Error> {
    // Step 0: extract the JSON payload (strips leading/trailing junk like </tool_call>).
    let cleaned = extract_json_payload(raw).unwrap_or(raw);
    let was_extracted = cleaned.len() != raw.len();

    // Fast path: valid JSON.
    if let Ok(value) = serde_json::from_str::<Value>(cleaned) {
        return Ok(ParsedToolArgs {
            value,
            was_repaired: was_extracted,
        });
    }

    // Targeted repair: escape literal control characters inside string values.
    // This is a well-scoped fix for models that emit raw newlines/tabs in JSON strings.
    if let Some(repaired) = repair_control_chars(cleaned)
        && let Ok(value) = serde_json::from_str::<Value>(&repaired)
    {
        warn!(
            "runtime: repaired control characters in JSON arguments\n  raw: {}",
            preview_text(raw, 200),
        );
        return Ok(ParsedToolArgs {
            value,
            was_repaired: true,
        });
    }

    // No more heuristic repairs. Return the parse error so the model
    // gets actionable feedback and can retry with valid JSON.
    serde_json::from_str::<Value>(cleaned).map(|value| ParsedToolArgs {
        value,
        was_repaired: false,
    })
}

pub(super) fn canonical_tool_args(value: &Value, fallback: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| fallback.to_string())
}

/// Extract the first top-level JSON object or array from `raw`, ignoring
/// leading/trailing junk (e.g. `</tool_call>` suffixes the model sometimes
/// appends).  Returns `None` if no `{` or `[` is found.
pub(super) fn extract_json_payload(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    // Find the first `{` or `[`.
    let open_pos = bytes.iter().position(|&b| b == b'{' || b == b'[')?;
    let open_char = bytes[open_pos];
    let close_char = if open_char == b'{' { b'}' } else { b']' };

    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut prev_backslash = false;
    let mut end_pos = None;

    for (i, &b) in bytes.iter().enumerate().skip(open_pos) {
        if in_string {
            if prev_backslash {
                prev_backslash = false;
                continue;
            }
            if b == b'\\' {
                prev_backslash = true;
            } else if b == b'"' {
                in_string = false;
            }
            // Inside strings, control characters don't affect depth tracking.
            continue;
        }
        match b {
            b'"' => in_string = true,
            b if b == open_char => depth += 1,
            b if b == close_char => {
                depth -= 1;
                if depth == 0 {
                    end_pos = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    let end = end_pos.unwrap_or(bytes.len().saturating_sub(1));
    let slice = &raw[open_pos..=end];
    let trailing = raw[end + 1..].trim_start();
    // Reject ambiguous concatenated payloads like `{"a":1}{"b":2}`.
    if trailing.starts_with('{') || trailing.starts_with('[') {
        return None;
    }

    // Only return if we actually trimmed something; avoids allocation.
    if open_pos == 0 && end == bytes.len() - 1 {
        None // Already the whole string, no extraction needed.
    } else {
        Some(slice)
    }
}

/// Escape literal control characters (0x00-0x1F except `\n`, `\r`, `\t`
/// which get standard escapes) that appear inside JSON string values.
/// The model sometimes emits actual newline bytes instead of `\n`.
pub(super) fn repair_control_chars(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut result = String::with_capacity(raw.len() + 64);
    let mut in_string = false;
    let mut prev_backslash = false;
    let mut changed = false;

    for &b in bytes {
        if in_string {
            if prev_backslash {
                prev_backslash = false;
                result.push(b as char);
                continue;
            }
            if b == b'\\' {
                prev_backslash = true;
                result.push('\\');
                continue;
            }
            if b == b'"' {
                in_string = false;
                result.push('"');
                continue;
            }
            // Escape control characters inside strings.
            if b < 0x20 {
                changed = true;
                match b {
                    b'\n' => result.push_str("\\n"),
                    b'\r' => result.push_str("\\r"),
                    b'\t' => result.push_str("\\t"),
                    _ => {
                        // Generic \u00XX escape.
                        result.push_str(&format!("\\u{:04x}", b));
                    }
                }
                continue;
            }
            result.push(b as char);
        } else {
            if b == b'"' {
                in_string = true;
            }
            result.push(b as char);
        }
    }

    if changed { Some(result) } else { None }
}

/// Close unterminated JSON containers/strings without synthesizing content.
/// This intentionally only appends missing terminators (`"`, `}`, `]`).
pub(super) fn repair_truncated_json_closure(raw: &str) -> Option<String> {
    let trimmed = raw.trim_end();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return None;
    }

    let bytes = trimmed.as_bytes();
    let mut expected_closers: Vec<u8> = Vec::new();
    let mut in_string = false;
    let mut prev_backslash = false;

    for &b in bytes {
        if in_string {
            if prev_backslash {
                prev_backslash = false;
                continue;
            }
            if b == b'\\' {
                prev_backslash = true;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            continue;
        }

        match b {
            b'"' => in_string = true,
            b'{' => expected_closers.push(b'}'),
            b'[' => expected_closers.push(b']'),
            b'}' | b']' => {
                let expected = expected_closers.pop()?;
                if expected != b {
                    return None;
                }
            }
            _ => {}
        }
    }

    // A dangling escape cannot be repaired safely without synthesizing content.
    if in_string && prev_backslash {
        return None;
    }

    let mut repaired = trimmed.to_string();
    let original_len = repaired.len();

    if in_string {
        repaired.push('"');
    }
    for closer in expected_closers.iter().rev() {
        repaired.push(*closer as char);
    }

    if repaired.len() == original_len {
        return None;
    }

    let delta = repaired.len() - original_len;
    let max_delta = std::cmp::max(8, original_len.saturating_mul(15) / 100);
    if delta > max_delta {
        return None;
    }

    Some(repaired)
}

/// Remove invalid `\$` JSON escapes that models sometimes add when trying to
/// preserve shell variables like `$i` or `$(...)` inside bash command strings.
pub(super) fn repair_invalid_dollar_escapes(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut result = String::with_capacity(raw.len());
    let mut in_string = false;
    let mut prev_backslash = false;
    let mut changed = false;

    for (idx, &b) in bytes.iter().enumerate() {
        if in_string {
            if prev_backslash {
                prev_backslash = false;
                result.push(b as char);
                continue;
            }
            if b == b'\\' {
                if bytes.get(idx + 1) == Some(&b'$') {
                    changed = true;
                    continue;
                }
                prev_backslash = true;
                result.push('\\');
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            result.push(b as char);
            continue;
        }

        if b == b'"' {
            in_string = true;
        }
        result.push(b as char);
    }

    if changed { Some(result) } else { None }
}

/// Escape unescaped `"` that appear *inside* JSON string values.
/// A `"` is treated as a real string terminator only if the next
/// non-whitespace byte is one of: `,`, `}`, `]`, `:`, or end-of-input.
pub(super) fn repair_unescaped_inner_quotes(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut result: Vec<u8> = Vec::with_capacity(bytes.len() + 16);
    let mut in_string = false;
    let mut prev_backslash = false;
    let mut changed = false;

    for (idx, &b) in bytes.iter().enumerate() {
        if in_string {
            if prev_backslash {
                prev_backslash = false;
                result.push(b);
                continue;
            }
            if b == b'\\' {
                prev_backslash = true;
                result.push(b);
                continue;
            }
            if b == b'"' {
                if is_probable_string_terminator(bytes, idx) {
                    in_string = false;
                    result.push(b);
                } else {
                    changed = true;
                    result.push(b'\\');
                    result.push(b'"');
                }
                continue;
            }
            result.push(b);
            continue;
        }

        if b == b'"' {
            in_string = true;
        }
        result.push(b);
    }

    if !changed {
        return None;
    }

    String::from_utf8(result).ok()
}

fn is_probable_string_terminator(bytes: &[u8], quote_idx: usize) -> bool {
    let mut idx = quote_idx.saturating_add(1);
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    if idx >= bytes.len() {
        return true;
    }

    matches!(bytes[idx], b',' | b'}' | b']' | b':')
}

/// Safe UTF-8 text preview for logging.
pub(super) fn preview_text(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &text[..byte_idx],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_or_repair_json ──

    #[test]
    fn parse_or_repair_json_valid_json_fast_path() {
        // Line 74: normal successful parse (no extraction, no repair).
        let raw = r#"{"key": "value"}"#;
        let result = parse_or_repair_json(raw).unwrap();
        assert!(!result.was_repaired);
        assert_eq!(result.value["key"], "value");
    }

    #[test]
    fn parse_or_repair_json_repairs_control_chars_in_strings() {
        // Line 63: control-character repair retry path — first parse fails,
        // repair_control_chars fixes literal newline/tab inside a string value.
        let raw = "{\"command\": \"echo\thello\nworld\"}";
        let result = parse_or_repair_json(raw).unwrap();
        assert!(result.was_repaired);
        assert!(result.value["command"].as_str().unwrap().contains("hello"));
    }

    #[test]
    fn parse_or_repair_json_returns_error_for_truly_broken_json() {
        // Neither control-char repair nor extraction can save this.
        let raw = "{{{{not json at all";
        assert!(parse_or_repair_json(raw).is_err());
    }

    // ── repair_control_chars ──

    #[test]
    fn repair_control_chars_escapes_tab_newline_cr() {
        // Lines 173-175: \n → \\n, \r → \\r, \t → \\t
        let raw = "{\"a\": \"line1\nline2\rline3\tend\"}";
        let repaired = repair_control_chars(raw).unwrap();
        assert!(repaired.contains("\\n"));
        assert!(repaired.contains("\\r"));
        assert!(repaired.contains("\\t"));
        // Should now be valid JSON.
        serde_json::from_str::<serde_json::Value>(&repaired).unwrap();
    }

    #[test]
    fn repair_control_chars_escapes_generic_control_char() {
        // Lines 176-178: generic \u00XX escape for control chars other than \n/\r/\t.
        // Use 0x01 (SOH) inside a string.
        let raw = format!("{{\"a\": \"hello{}world\"}}", '\x01');
        let repaired = repair_control_chars(&raw).unwrap();
        assert!(repaired.contains("\\u0001"));
        serde_json::from_str::<serde_json::Value>(&repaired).unwrap();
    }

    #[test]
    fn repair_control_chars_returns_none_when_no_change() {
        let raw = r#"{"key": "value"}"#;
        assert!(repair_control_chars(raw).is_none());
    }

    // ── repair_truncated_json_closure ──

    #[test]
    fn repair_truncated_closure_closes_missing_brace() {
        // Lines 200-254: truncated JSON with missing closing brace.
        let raw = r#"{"command": "ls""#;
        let repaired = repair_truncated_json_closure(raw).unwrap();
        serde_json::from_str::<serde_json::Value>(&repaired).unwrap();
    }

    #[test]
    fn repair_truncated_closure_closes_missing_bracket() {
        // Line 227: array bracket matching.
        let raw = r#"[{"a": 1}, {"b": 2}"#;
        let repaired = repair_truncated_json_closure(raw).unwrap();
        serde_json::from_str::<serde_json::Value>(&repaired).unwrap();
    }

    #[test]
    fn repair_truncated_closure_deeply_nested() {
        // Lines 211-216: deeply nested with backslash inside string.
        let raw = r#"{"a": {"b": {"c": "val with \" escaped""#;
        let repaired = repair_truncated_json_closure(raw).unwrap();
        serde_json::from_str::<serde_json::Value>(&repaired).unwrap();
    }

    #[test]
    fn repair_truncated_closure_unterminated_string() {
        // Truncated mid-string: should close the string then the braces.
        let raw = r#"{"command": "echo hello"#;
        let repaired = repair_truncated_json_closure(raw).unwrap();
        serde_json::from_str::<serde_json::Value>(&repaired).unwrap();
    }

    #[test]
    fn repair_truncated_closure_returns_none_for_non_json() {
        // Line 200: input doesn't start with { or [.
        assert!(repair_truncated_json_closure("not json").is_none());
    }

    #[test]
    fn repair_truncated_closure_returns_none_when_already_complete() {
        // Line 253: no delta means nothing to repair.
        let raw = r#"{"key": "value"}"#;
        assert!(repair_truncated_json_closure(raw).is_none());
    }

    #[test]
    fn repair_truncated_closure_returns_none_for_dangling_backslash() {
        // Line 240: in_string && prev_backslash → None.
        let raw = r#"{"a": "val\"#;
        assert!(repair_truncated_json_closure(raw).is_none());
    }

    #[test]
    fn repair_truncated_closure_returns_none_when_delta_too_large() {
        // Line 260: delta exceeds max_delta (15% of length or 8).
        // A very short payload missing many closers.
        let raw = r#"{"a":[[[[[[[[[["#;
        assert!(repair_truncated_json_closure(raw).is_none());
    }

    #[test]
    fn repair_truncated_closure_returns_none_for_mismatched_brackets() {
        // Mismatch: open { but see ] → returns None from the pop check.
        let raw = r#"{"a": 1]"#;
        assert!(repair_truncated_json_closure(raw).is_none());
    }

    // ── repair_invalid_dollar_escapes ──

    #[test]
    fn repair_dollar_escapes_removes_backslash_before_dollar() {
        let raw = r#"{"command": "echo \$HOME"}"#;
        let repaired = repair_invalid_dollar_escapes(raw).unwrap();
        assert!(repaired.contains("$HOME"));
        assert!(!repaired.contains(r"\$HOME"));
        serde_json::from_str::<serde_json::Value>(&repaired).unwrap();
    }

    #[test]
    fn repair_dollar_escapes_returns_none_when_no_change() {
        let raw = r#"{"command": "echo $HOME"}"#;
        assert!(repair_invalid_dollar_escapes(raw).is_none());
    }

    // ── repair_unescaped_inner_quotes ──

    #[test]
    fn repair_unescaped_inner_quotes_escapes_inner_quotes() {
        let raw = r#"{"command": "echo "hello" world"}"#;
        let repaired = repair_unescaped_inner_quotes(raw).unwrap();
        serde_json::from_str::<serde_json::Value>(&repaired).unwrap();
    }

    #[test]
    fn repair_unescaped_inner_quotes_returns_none_when_no_change() {
        let raw = r#"{"key": "value"}"#;
        assert!(repair_unescaped_inner_quotes(raw).is_none());
    }

    #[test]
    fn is_probable_string_terminator_at_end_of_input() {
        // Line 363: idx >= bytes.len() → true (quote at end of input).
        let raw = r#"{"a": "hello"}"#;
        // The last `"` before `}` is a terminator; test the function directly.
        let bytes = raw.as_bytes();
        // Find the quote right before the final `}`.
        let last_quote = raw.rfind('"').unwrap();
        assert!(is_probable_string_terminator(bytes, last_quote));
    }

    #[test]
    fn is_probable_string_terminator_returns_true_at_eof() {
        // Line 363: quote at exact end of input.
        let raw = r#""hello""#;
        let bytes = raw.as_bytes();
        let last = raw.len() - 1;
        assert!(is_probable_string_terminator(bytes, last));
    }

    // ── Combined repair scenarios ──

    #[test]
    fn combined_control_chars_and_extraction() {
        // JSON with trailing junk and control chars.
        let raw = "{\"a\": \"val\tue\"}</tool_call>";
        let result = parse_or_repair_json(raw).unwrap();
        assert!(result.was_repaired);
    }

    #[test]
    fn extract_json_payload_strips_leading_trailing_text() {
        let raw = r#"some junk {"key": "value"} more junk"#;
        let extracted = extract_json_payload(raw).unwrap();
        serde_json::from_str::<serde_json::Value>(extracted).unwrap();
    }

    #[test]
    fn extract_json_payload_returns_none_for_concatenated() {
        let raw = r#"{"a":1}{"b":2}"#;
        assert!(extract_json_payload(raw).is_none());
    }

    #[test]
    fn strip_thinking_tags_removes_think_block() {
        let raw = r#"<think>reasoning here</think>{"key": "value"}"#;
        let cleaned = strip_thinking_tags(raw);
        assert!(cleaned.starts_with('{'));
    }

    #[test]
    fn strip_thinking_tags_extracts_json_from_prose() {
        let raw = r#"Here is the JSON: {"key": "value"}"#;
        let cleaned = strip_thinking_tags(raw);
        assert!(cleaned.starts_with('{'));
    }
}
