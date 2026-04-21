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

/// Safe UTF-8 text preview for logging.
pub(super) fn preview_text(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &text[..byte_idx],
        None => text,
    }
}
