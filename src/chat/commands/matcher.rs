use super::types::CommandSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchTier {
    Exact,
    Prefix,
    Contains,
    Fuzzy,
}

pub fn ranked_match_names<'a>(query: &str, specs: &'a [CommandSpec]) -> Vec<&'a str> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() || needle == "/" {
        return specs.iter().map(|spec| spec.name.as_str()).collect();
    }

    let mut scored: Vec<(MatchTier, usize, &'a str)> = specs
        .iter()
        .enumerate()
        .filter_map(|(index, spec)| {
            score_spec(&needle, spec).map(|tier| (tier, index, spec.name.as_str()))
        })
        .collect();

    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .map(|(_, _, command_name)| command_name)
        .collect()
}

fn score_spec(needle: &str, spec: &CommandSpec) -> Option<MatchTier> {
    let name = spec.name.to_lowercase();
    let description = spec.description.to_lowercase();

    if name == needle {
        Some(MatchTier::Exact)
    } else if name.starts_with(needle) {
        Some(MatchTier::Prefix)
    } else if name.contains(needle) || description.contains(needle) {
        Some(MatchTier::Contains)
    } else if is_subsequence(needle, &name) {
        Some(MatchTier::Fuzzy)
    } else {
        None
    }
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    if needle.is_empty() {
        return true;
    }

    let mut needle_chars = needle.chars();
    let mut current = needle_chars.next();

    for hay in haystack.chars() {
        if current == Some(hay) {
            current = needle_chars.next();
            if current.is_none() {
                return true;
            }
        }
    }

    false
}
