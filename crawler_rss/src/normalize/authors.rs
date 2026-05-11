use std::collections::HashSet;

use feed_rs::model::Entry;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use super::author_patterns::{
    author_conjunction_split_regex, author_list_split_regex, domain_fragment_regex,
    generic_editorial_label_regex, initial_token_regex, inline_editorial_separator_regexes,
    leading_byline_regex, leading_editorial_prefix_regexes, leading_with_regex, name_word_regex,
    parenthetical_chunk_regex, role_label_regex, role_location_regex, trailing_location_regex,
    trailing_role_regex, trailing_with_regex,
};
use super::author_tokens::{is_descriptor_cutoff_token, is_location_preposition, is_name_particle};
use super::text::normalize_required_text;

pub(super) fn extract_authors(entry: &Entry) -> Vec<String> {
    let mut authors = Vec::new();
    let mut seen_normalized_names = HashSet::new();

    for author in &entry.authors {
        for candidate in split_author_value(&author.name) {
            let Some(normalized_name) = normalize_author_identity(&candidate) else {
                continue;
            };
            if seen_normalized_names.insert(normalized_name) {
                authors.push(candidate);
            }
        }
    }

    authors
}

fn split_author_value(value: &str) -> Vec<String> {
    let Some(normalized_value) = normalize_required_text(value) else {
        return Vec::new();
    };

    let mut authors = Vec::new();
    let mut seen_normalized_names = HashSet::new();

    for part in split_author_candidates(&normalized_value) {
        let Some(candidate) = clean_author_candidate(&part) else {
            continue;
        };
        let Some(normalized_name) = normalize_author_identity(&candidate) else {
            continue;
        };
        if seen_normalized_names.insert(normalized_name) {
            authors.push(candidate);
        }
    }

    authors
}

fn split_author_candidates(value: &str) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    let prepared_value = prepare_author_source_value(value);

    for part in author_list_split_regex().split(&prepared_value) {
        let Some(display_part) = normalize_display_name(part) else {
            continue;
        };
        for conjunction_part in split_conjunction_candidates(&display_part) {
            let Some(cleaned_candidate) = clean_author_candidate(&conjunction_part) else {
                continue;
            };
            if starts_with_role_label(&cleaned_candidate)
                && candidates
                    .iter()
                    .any(|existing_candidate| !starts_with_role_label(existing_candidate))
            {
                continue;
            }
            candidates.push(cleaned_candidate);
        }
    }

    candidates
}

fn split_conjunction_candidates(value: &str) -> Vec<String> {
    let parts = author_conjunction_split_regex()
        .split(value)
        .filter_map(normalize_display_name)
        .collect::<Vec<_>>();
    if parts.len() <= 1 {
        return vec![value.to_string()];
    }

    let cleaned_parts = parts
        .iter()
        .filter_map(|part| clean_author_candidate(part))
        .collect::<Vec<_>>();
    if cleaned_parts.len() != parts.len()
        || !cleaned_parts
            .iter()
            .all(|part| looks_like_standalone_author(part))
    {
        return vec![value.to_string()];
    }

    parts
}

fn clean_author_candidate(value: &str) -> Option<String> {
    let mut candidate = normalize_display_name(value)?;
    candidate = strip_leading_editorial_prefixes(&candidate)?;
    candidate = leading_byline_regex()
        .replace_all(&candidate, "")
        .into_owned();
    candidate = normalize_display_name(&candidate)?;
    candidate = strip_parenthetical_chunks(&candidate)?;

    if let Some(captures) = leading_with_regex().captures(&candidate) {
        candidate = captures.get(1)?.as_str().trim().to_string();
    }

    if let Some(captures) = trailing_with_regex().captures(&candidate) {
        let left_candidate = captures
            .get(1)
            .and_then(|matched| normalize_display_name(matched.as_str()));
        let right_candidate = captures
            .get(2)
            .and_then(|matched| normalize_display_name(matched.as_str()));

        for preferred_candidate in [left_candidate, right_candidate] {
            let Some(preferred_candidate) = preferred_candidate else {
                continue;
            };
            if !is_discardable_author_fragment(&preferred_candidate) {
                candidate = preferred_candidate;
                break;
            }
        }
    }

    if let Some(captures) = trailing_role_regex().captures(&candidate) {
        let left_candidate = captures
            .get(1)
            .and_then(|matched| normalize_display_name(matched.as_str()));
        if let Some(left_candidate) = left_candidate {
            if looks_like_named_author(&left_candidate) {
                candidate = left_candidate;
            }
        }
    }

    if !starts_with_role_label(&candidate) {
        if let Some(captures) = trailing_location_regex().captures(&candidate) {
            let left_candidate = captures
                .get(1)
                .and_then(|matched| normalize_display_name(matched.as_str()));
            if let Some(left_candidate) = left_candidate {
                if looks_like_named_author(&left_candidate) {
                    candidate = left_candidate;
                }
            }
        }
    }

    if !starts_with_role_label(&candidate) && has_descriptor_cutoff_cue(&candidate) {
        candidate = extract_leading_name_fragment(&candidate).unwrap_or(candidate);
    }

    candidate = normalize_display_name(&candidate)?;
    if is_discardable_author_fragment(&candidate) {
        return None;
    }
    Some(candidate)
}

fn normalize_display_name(value: &str) -> Option<String> {
    let candidate = value.trim().trim_matches('"').trim_matches('\'');
    if candidate.is_empty() {
        return None;
    }
    Some(candidate.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn normalize_author_identity(value: &str) -> Option<String> {
    let mut normalized = String::with_capacity(value.len());
    let mut last_was_space = false;

    for character in value.nfkd() {
        if is_combining_mark(character) {
            continue;
        }
        for lowered in character.to_lowercase() {
            if lowered.is_ascii_alphanumeric() {
                normalized.push(lowered);
                last_was_space = false;
                continue;
            }
            if !last_was_space {
                normalized.push(' ');
                last_was_space = true;
            }
        }
    }

    let collapsed = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    Some(collapsed)
}

fn looks_like_standalone_author(value: &str) -> bool {
    if is_discardable_author_fragment(value) {
        return false;
    }

    if value.split_whitespace().count() >= 2 {
        return true;
    }

    let compact_value = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>();
    !compact_value.is_empty()
        && compact_value.len() <= 10
        && value
            .chars()
            .all(|character| !character.is_alphabetic() || character.is_uppercase())
}

fn looks_like_named_author(value: &str) -> bool {
    if starts_with_role_label(value) {
        return false;
    }
    looks_like_standalone_author(value)
}

fn is_discardable_author_fragment(value: &str) -> bool {
    matches!(normalize_author_identity(value).as_deref(), Some("text"))
        || (starts_with_role_label(value) && !role_location_regex().is_match(value))
        || generic_editorial_label_regex().is_match(value)
        || contains_domain_fragment(value)
        || is_location_fragment(value)
}

fn contains_domain_fragment(value: &str) -> bool {
    domain_fragment_regex().is_match(value)
}

fn is_location_fragment(value: &str) -> bool {
    let mut parts = value.split_whitespace();
    let Some(first_token) = parts.next() else {
        return false;
    };
    if parts.next().is_none() || first_token != first_token.to_lowercase() {
        return false;
    }

    matches!(
        normalize_author_identity(first_token).as_deref(),
        Some(token) if is_location_preposition(token)
    )
}

fn starts_with_role_label(value: &str) -> bool {
    role_label_regex().is_match(value)
}

fn strip_leading_editorial_prefixes(value: &str) -> Option<String> {
    let mut candidate = value.to_string();
    loop {
        let mut updated_candidate = candidate.clone();
        for regex in leading_editorial_prefix_regexes() {
            updated_candidate = regex.replace_all(&updated_candidate, "").trim().to_string();
        }
        if updated_candidate == candidate {
            return normalize_display_name(&updated_candidate);
        }
        candidate = updated_candidate;
    }
}

fn prepare_author_source_value(value: &str) -> String {
    let mut prepared_value = value.to_string();
    for regex in inline_editorial_separator_regexes() {
        prepared_value = regex.replace_all(&prepared_value, " ; ").into_owned();
    }
    prepared_value
}

fn strip_parenthetical_chunks(value: &str) -> Option<String> {
    let mut candidate = value.to_string();
    loop {
        let updated_candidate = parenthetical_chunk_regex()
            .replace_all(&candidate, "")
            .trim()
            .to_string();
        if updated_candidate == candidate {
            return normalize_display_name(&updated_candidate);
        }
        candidate = updated_candidate;
    }
}

fn has_descriptor_cutoff_cue(value: &str) -> bool {
    let normalized_tokens = value
        .split_whitespace()
        .filter_map(normalize_author_identity)
        .collect::<Vec<_>>();
    if normalized_tokens.len() <= 1 {
        return false;
    }

    normalized_tokens
        .iter()
        .skip(1)
        .any(|token| is_descriptor_cutoff_token(token))
}

fn extract_leading_name_fragment(value: &str) -> Option<String> {
    let mut extracted_tokens = Vec::new();
    let mut saw_name_token = false;

    for token in value.split_whitespace() {
        let cleaned_token =
            token.trim_matches(|character| matches!(character, ',' | ';' | ':' | '/'));
        if cleaned_token.is_empty() {
            continue;
        }

        let Some(normalized_token) = normalize_author_identity(cleaned_token) else {
            break;
        };
        if is_descriptor_cutoff_token(&normalized_token) {
            break;
        }
        if is_name_particle(&normalized_token) && !extracted_tokens.is_empty() {
            extracted_tokens.push(cleaned_token.to_string());
            continue;
        }
        if is_name_like_token(cleaned_token) {
            extracted_tokens.push(cleaned_token.to_string());
            saw_name_token = true;
            continue;
        }
        break;
    }

    while let Some(last_token) = extracted_tokens.last() {
        let trailing_is_particle = normalize_author_identity(last_token)
            .map(|token| is_name_particle(&token))
            .unwrap_or(false);
        if !trailing_is_particle {
            break;
        }
        extracted_tokens.pop();
    }

    if !saw_name_token {
        return None;
    }
    normalize_display_name(&extracted_tokens.join(" "))
}

fn is_name_like_token(value: &str) -> bool {
    initial_token_regex().is_match(value) || name_word_regex().is_match(value)
}

#[cfg(test)]
#[path = "authors_tests.rs"]
mod tests;
