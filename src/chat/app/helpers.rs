//! Pure utility functions used by `ChatApp` and its submodules.

/// Shorten `/home/<user>/...` to `~/...` for display.
pub(crate) fn shorten_home(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME")
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path.to_owned()
}

pub(crate) fn normalize_slash_query(query: &str) -> String {
    query
        .replace(['\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn infer_pr_label_from_branch(branch: &str) -> Option<String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return None;
    }

    let segments = branch
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    for (index, segment) in segments.iter().enumerate() {
        if let Some(number) = segment.strip_prefix("pr-").and_then(leading_digits) {
            return Some(format!("PR#{number}"));
        }
        if let Some(number) = segment.strip_prefix("pr_").and_then(leading_digits) {
            return Some(format!("PR#{number}"));
        }
        if matches!(*segment, "pr" | "pull")
            && let Some(number) = segments
                .get(index + 1)
                .and_then(|next| leading_digits(next))
        {
            return Some(format!("PR#{number}"));
        }
    }

    let hash_suffix = branch.rsplit_once('#').map(|(_, suffix)| suffix)?;
    let number = leading_digits(hash_suffix)?;
    Some(format!("PR#{number}"))
}

fn leading_digits(value: &str) -> Option<&str> {
    let end = value
        .char_indices()
        .find_map(|(index, ch)| (!ch.is_ascii_digit()).then_some(index))
        .unwrap_or(value.len());
    (end > 0).then_some(&value[..end])
}

pub(super) fn parse_workspace_claim_path(claim: &str) -> Option<&str> {
    let (resource, _) = claim.split_once('#')?;
    let path = resource.strip_prefix("fs:")?;
    (!path.is_empty()).then_some(path)
}

pub(super) fn preview_text(text: &str, max_chars: usize) -> &str {
    if text.chars().count() <= max_chars {
        return text;
    }

    let end = text
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| index)
        .unwrap_or(text.len());

    &text[..end]
}

pub(super) fn summarize_tool_arguments(arguments: &str) -> String {
    preview_text(arguments, 60).to_owned()
}

pub(super) fn summarize_tool_output(output: &str) -> String {
    let first_line = output.lines().next().unwrap_or("");
    preview_text(first_line, 80).to_owned()
}

/// Detect the current git branch. Returns empty string if not in a git repo.
pub(super) fn detect_branch(repo_path: &str) -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_owned())
            } else {
                None
            }
        })
        .unwrap_or_default()
}
