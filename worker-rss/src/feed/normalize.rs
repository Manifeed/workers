use std::collections::HashSet;
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use feed_rs::model::Entry;
use regex::Regex;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use crate::model::RssSource;

const SUMMARY_BREAK_TAGS: &[&str] = &[
    "article",
    "aside",
    "blockquote",
    "br",
    "div",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "img",
    "li",
    "ol",
    "p",
    "section",
    "tr",
    "ul",
];

pub(super) fn normalize_sources(
    parsed_feed: &feed_rs::model::Feed,
    published_since: Option<DateTime<Utc>>,
) -> Vec<RssSource> {
    let mut seen_urls = HashSet::new();
    let mut sources = Vec::new();

    for entry in &parsed_feed.entries {
        let Some(link) = entry
            .links
            .iter()
            .find_map(|link| normalize_required_text(&link.href))
        else {
            continue;
        };
        if !seen_urls.insert(link.clone()) {
            continue;
        }

        let published_at = entry.published.or(entry.updated);
        if let Some(published_since) = published_since {
            if published_at
                .map(|published_at| published_at < published_since)
                .unwrap_or(true)
            {
                continue;
            }
        } else if published_at.is_none() {
            continue;
        }

        let Some(title) = entry
            .title
            .as_ref()
            .and_then(|title| normalize_required_text(&title.content))
        else {
            continue;
        };

        sources.push(RssSource {
            title,
            url: link,
            summary: entry
                .summary
                .as_ref()
                .and_then(|summary| clean_summary_text(&summary.content))
                .or_else(|| {
                    entry
                        .content
                        .as_ref()
                        .and_then(|content| content.body.as_deref())
                        .and_then(clean_summary_text)
                }),
            authors: extract_authors(entry),
            published_at,
            image_url: extract_image_url(entry),
        });
    }

    sources
}

pub(super) fn extract_image_url(entry: &Entry) -> Option<String> {
    extract_media_thumbnail_url(entry)
        .or_else(|| extract_media_content_image_url(entry))
        .or_else(|| extract_inline_image_url(entry))
}

fn extract_media_thumbnail_url(entry: &Entry) -> Option<String> {
    entry
        .media
        .iter()
        .flat_map(|media| media.thumbnails.iter())
        .map(|thumbnail| thumbnail.image.uri.clone())
        .find(|uri| looks_like_image_url(uri))
}

fn extract_media_content_image_url(entry: &Entry) -> Option<String> {
    entry
        .media
        .iter()
        .flat_map(|media| media.content.iter())
        .find_map(|content| {
            let url = content.url.as_ref()?.as_str().to_string();
            if content
                .content_type
                .as_ref()
                .map(|content_type| content_type.as_str().starts_with("image/"))
                .unwrap_or_else(|| looks_like_image_url(&url))
            {
                Some(url)
            } else {
                None
            }
        })
}

fn extract_inline_image_url(entry: &Entry) -> Option<String> {
    entry
        .summary
        .as_ref()
        .and_then(|summary| find_first_image_src(&summary.content))
        .or_else(|| {
            entry.content.as_ref().and_then(|content| {
                content
                    .body
                    .as_deref()
                    .and_then(find_first_image_src)
                    .or_else(|| {
                        content
                            .src
                            .as_ref()
                            .map(|src| src.href.clone())
                            .filter(|src| looks_like_image_url(src))
                    })
            })
        })
}

fn find_first_image_src(html: &str) -> Option<String> {
    let lower_html = html.to_ascii_lowercase();
    let img_index = lower_html.find("<img")?;
    let src_index = lower_html[img_index..].find("src=")? + img_index + 4;
    let quote = html[src_index..].chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }

    let value_start = src_index + quote.len_utf8();
    let value_end = html[value_start..].find(quote)? + value_start;
    let candidate = html[value_start..value_end].trim();
    if candidate.is_empty() {
        return None;
    }

    let decoded = html_escape::decode_html_entities(candidate).into_owned();
    if looks_like_image_url(&decoded) {
        Some(decoded)
    } else {
        None
    }
}

fn looks_like_image_url(value: &str) -> bool {
    let lower_value = value.to_ascii_lowercase();
    lower_value.starts_with("http://")
        || lower_value.starts_with("https://")
        || lower_value.starts_with("//")
        || lower_value.contains(".jpg")
        || lower_value.contains(".jpeg")
        || lower_value.contains(".png")
        || lower_value.contains(".webp")
        || lower_value.contains(".gif")
        || lower_value.contains(".avif")
}

pub(super) fn clean_summary_text(raw: &str) -> Option<String> {
    let mut cleaned = strip_html_tags(raw);
    for _ in 0..2 {
        let decoded = html_escape::decode_html_entities(&cleaned).into_owned();
        if decoded == cleaned {
            break;
        }
        cleaned = strip_html_tags(&decoded);
    }

    let cleaned = trim_boundary_quotes(&collapse_whitespace(&cleaned));
    if cleaned.is_empty() {
        return None;
    }

    Some(cleaned)
}

fn normalize_required_text(value: &str) -> Option<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return None;
    }
    Some(normalized.to_string())
}

fn extract_authors(entry: &Entry) -> Vec<String> {
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

fn author_list_split_regex() -> &'static Regex {
    static AUTHOR_LIST_SPLIT_REGEX: OnceLock<Regex> = OnceLock::new();
    AUTHOR_LIST_SPLIT_REGEX
        .get_or_init(|| Regex::new(r"\s*[;,]\s*").expect("author list split regex must be valid"))
}

fn author_conjunction_split_regex() -> &'static Regex {
    static AUTHOR_CONJUNCTION_SPLIT_REGEX: OnceLock<Regex> = OnceLock::new();
    AUTHOR_CONJUNCTION_SPLIT_REGEX.get_or_init(|| {
        Regex::new(r"(?i)\s*(?:&|\band\b|\bet\b)\s*")
            .expect("author conjunction split regex must be valid")
    })
}

fn leading_byline_regex() -> &'static Regex {
    static LEADING_BYLINE_REGEX: OnceLock<Regex> = OnceLock::new();
    LEADING_BYLINE_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(?:(?:par|by)\s+)+").expect("leading byline regex must be valid")
    })
}

fn inline_editorial_separator_regexes() -> [&'static Regex; 3] {
    [
        edited_by_separator_regex(),
        executive_producers_are_separator_regex(),
        executive_producer_is_separator_regex(),
    ]
}

fn edited_by_separator_regex() -> &'static Regex {
    static EDITED_BY_SEPARATOR_REGEX: OnceLock<Regex> = OnceLock::new();
    EDITED_BY_SEPARATOR_REGEX
        .get_or_init(|| Regex::new(r"(?i)\bedited by\b").expect("edited by regex must be valid"))
}

fn executive_producers_are_separator_regex() -> &'static Regex {
    static EXECUTIVE_PRODUCERS_ARE_SEPARATOR_REGEX: OnceLock<Regex> = OnceLock::new();
    EXECUTIVE_PRODUCERS_ARE_SEPARATOR_REGEX.get_or_init(|| {
        Regex::new(r"(?i)\bexecutive producers? are\b")
            .expect("executive producers are regex must be valid")
    })
}

fn executive_producer_is_separator_regex() -> &'static Regex {
    static EXECUTIVE_PRODUCER_IS_SEPARATOR_REGEX: OnceLock<Regex> = OnceLock::new();
    EXECUTIVE_PRODUCER_IS_SEPARATOR_REGEX.get_or_init(|| {
        Regex::new(r"(?i)\bexecutive producer is\b")
            .expect("executive producer is regex must be valid")
    })
}

fn leading_editorial_prefix_regexes() -> [&'static Regex; 6] {
    [
        leading_de_notre_regex(),
        leading_notre_regex(),
        leading_propos_recueillis_regex(),
        leading_recueilli_par_regex(),
        leading_reported_by_regex(),
        leading_text_regex(),
    ]
}

fn leading_de_notre_regex() -> &'static Regex {
    static LEADING_DE_NOTRE_REGEX: OnceLock<Regex> = OnceLock::new();
    LEADING_DE_NOTRE_REGEX
        .get_or_init(|| Regex::new(r"(?i)^(?:de notre)\s+").expect("de notre regex must be valid"))
}

fn leading_propos_recueillis_regex() -> &'static Regex {
    static LEADING_PROPOS_RECUEILLIS_REGEX: OnceLock<Regex> = OnceLock::new();
    LEADING_PROPOS_RECUEILLIS_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(?:propos recueillis par)\s+")
            .expect("propos recueillis regex must be valid")
    })
}

fn leading_notre_regex() -> &'static Regex {
    static LEADING_NOTRE_REGEX: OnceLock<Regex> = OnceLock::new();
    LEADING_NOTRE_REGEX
        .get_or_init(|| Regex::new(r"(?i)^(?:notre)\s+").expect("notre regex must be valid"))
}

fn leading_recueilli_par_regex() -> &'static Regex {
    static LEADING_RECUEILLI_PAR_REGEX: OnceLock<Regex> = OnceLock::new();
    LEADING_RECUEILLI_PAR_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(?:recueilli(?:e|es|s)? par)\s+")
            .expect("recueilli par regex must be valid")
    })
}

fn leading_reported_by_regex() -> &'static Regex {
    static LEADING_REPORTED_BY_REGEX: OnceLock<Regex> = OnceLock::new();
    LEADING_REPORTED_BY_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(?:reported by)\s+").expect("reported by regex must be valid")
    })
}

fn leading_text_regex() -> &'static Regex {
    static LEADING_TEXT_REGEX: OnceLock<Regex> = OnceLock::new();
    LEADING_TEXT_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(?:text(?:\s+by)?|texte(?:\s+de)?)\s+").expect("text regex must be valid")
    })
}

fn leading_with_regex() -> &'static Regex {
    static LEADING_WITH_REGEX: OnceLock<Regex> = OnceLock::new();
    LEADING_WITH_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(?:avec|with)\s+(.+)$").expect("leading with regex must be valid")
    })
}

fn trailing_with_regex() -> &'static Regex {
    static TRAILING_WITH_REGEX: OnceLock<Regex> = OnceLock::new();
    TRAILING_WITH_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(.+?)\s+(?:avec|with)\s+(.+)$")
            .expect("trailing with regex must be valid")
    })
}

fn domain_fragment_regex() -> &'static Regex {
    static DOMAIN_FRAGMENT_REGEX: OnceLock<Regex> = OnceLock::new();
    DOMAIN_FRAGMENT_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(?:^|[\s(])(?:www\.)?[a-z0-9-]+\.(?:com|fr|org|net|io|co|info|tv|fm|be|ch|de|uk|eu)(?:$|[\s)])",
        )
        .expect("domain fragment regex must be valid")
    })
}

fn role_label_regex() -> &'static Regex {
    static ROLE_LABEL_REGEX: OnceLock<Regex> = OnceLock::new();
    ROLE_LABEL_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:correspondance|correspondant(?:e)?|correspondent|special correspondent|envoy(?:e|é)e?\s+sp(?:e|é)cial(?:e)?)\b",
        )
        .expect("role label regex must be valid")
    })
}

fn trailing_role_regex() -> &'static Regex {
    static TRAILING_ROLE_REGEX: OnceLock<Regex> = OnceLock::new();
    TRAILING_ROLE_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^(.+?)(?:\s*[,/-]\s*|\s+)(?:correspondance|correspondant(?:e)?|correspondent|special correspondent|envoy(?:e|é)e?\s+sp(?:e|é)cial(?:e)?)\b.*$",
        )
        .expect("trailing role regex must be valid")
    })
}

fn trailing_location_regex() -> &'static Regex {
    static TRAILING_LOCATION_REGEX: OnceLock<Regex> = OnceLock::new();
    TRAILING_LOCATION_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(.+?)(?:\s*[,/-]\s*|\s+)(?:in|at|a|à|au|aux|en|dans|depuis|sur)\s+.+$")
            .expect("trailing location regex must be valid")
    })
}

fn generic_editorial_label_regex() -> &'static Regex {
    static GENERIC_EDITORIAL_LABEL_REGEX: OnceLock<Regex> = OnceLock::new();
    GENERIC_EDITORIAL_LABEL_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:a|an|the|un|une|le|la|les)\s+.+\b(?:reporter|editor|staff|team|desk|bureau|newsroom|redaction|rédaction|editorial)\b.*$",
        )
        .expect("generic editorial label regex must be valid")
    })
}

fn role_location_regex() -> &'static Regex {
    static ROLE_LOCATION_REGEX: OnceLock<Regex> = OnceLock::new();
    ROLE_LOCATION_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:correspondance|correspondant(?:e)?|correspondent|special correspondent|envoy(?:e|é)e?\s+sp(?:e|é)cial(?:e)?)\s+(?:in|at|a|à|au|aux|en|dans|depuis|sur)\s+.+$",
        )
        .expect("role location regex must be valid")
    })
}

fn name_word_regex() -> &'static Regex {
    static NAME_WORD_REGEX: OnceLock<Regex> = OnceLock::new();
    NAME_WORD_REGEX.get_or_init(|| {
        Regex::new(r"^[A-Za-zÀ-ÖØ-öø-ÿ]+(?:[-'’][A-Za-zÀ-ÖØ-öø-ÿ]+)*$")
            .expect("name word regex must be valid")
    })
}

fn initial_token_regex() -> &'static Regex {
    static INITIAL_TOKEN_REGEX: OnceLock<Regex> = OnceLock::new();
    INITIAL_TOKEN_REGEX.get_or_init(|| {
        Regex::new(r"^(?:[A-Za-zÀ-ÖØ-öø-ÿ]\.)+(?:-[A-Za-zÀ-ÖØ-öø-ÿ]\.)*$")
            .expect("initial token regex must be valid")
    })
}

fn parenthetical_chunk_regex() -> &'static Regex {
    static PARENTHETICAL_CHUNK_REGEX: OnceLock<Regex> = OnceLock::new();
    PARENTHETICAL_CHUNK_REGEX.get_or_init(|| {
        Regex::new(r"\s*\([^()]*\)").expect("parenthetical chunk regex must be valid")
    })
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
        Some("in" | "at" | "a" | "au" | "aux" | "en" | "dans" | "depuis" | "sur")
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

    normalized_tokens.iter().skip(1).any(|token| {
        matches!(
            token.as_str(),
            "in" | "at"
                | "a"
                | "au"
                | "aux"
                | "en"
                | "dans"
                | "depuis"
                | "sur"
                | "aumonier"
                | "aumoniere"
                | "diocese"
                | "editor"
                | "editors"
                | "edited"
                | "executive"
                | "producer"
                | "producers"
                | "reporter"
                | "regional"
                | "regionale"
                | "special"
                | "speciale"
                | "speciales"
                | "specialiste"
                | "correspondance"
                | "correspondant"
                | "correspondante"
                | "hospital"
                | "hopital"
                | "bureau"
        )
    })
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
        if matches!(
            normalized_token.as_str(),
            "in" | "at"
                | "a"
                | "au"
                | "aux"
                | "en"
                | "dans"
                | "depuis"
                | "sur"
                | "aumonier"
                | "aumoniere"
                | "diocese"
                | "editor"
                | "editors"
                | "edited"
                | "executive"
                | "producer"
                | "producers"
                | "reporter"
                | "regional"
                | "regionale"
                | "special"
                | "speciale"
                | "speciales"
                | "specialiste"
                | "correspondance"
                | "correspondant"
                | "correspondante"
                | "hospital"
                | "hopital"
                | "bureau"
        ) {
            break;
        }
        if matches!(
            normalized_token.as_str(),
            "de" | "du"
                | "des"
                | "del"
                | "della"
                | "di"
                | "da"
                | "van"
                | "von"
                | "bin"
                | "ibn"
                | "al"
                | "la"
                | "le"
        ) && !extracted_tokens.is_empty()
        {
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
            .map(|token| {
                matches!(
                    token.as_str(),
                    "de" | "du"
                        | "des"
                        | "del"
                        | "della"
                        | "di"
                        | "da"
                        | "van"
                        | "von"
                        | "bin"
                        | "ibn"
                        | "al"
                        | "la"
                        | "le"
                )
            })
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

fn strip_html_tags(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut inside_tag = false;
    let mut tag_buffer = String::new();
    let mut index = 0;

    while index < chars.len() {
        let current = chars[index];
        if inside_tag {
            if current == '>' {
                inside_tag = false;
                if html_tag_creates_break(&tag_buffer) {
                    push_space_if_needed(&mut output);
                }
            } else {
                tag_buffer.push(current);
            }
            index += 1;
            continue;
        }

        if current == '<' && looks_like_html_tag(&chars, index) {
            inside_tag = true;
            tag_buffer.clear();
            index += 1;
            continue;
        }

        output.push(current);
        index += 1;
    }

    output
}

fn looks_like_html_tag(chars: &[char], index: usize) -> bool {
    matches!(
        chars.get(index + 1),
        Some(next) if next.is_ascii_alphabetic() || matches!(next, '/' | '!' | '?')
    )
}

fn html_tag_creates_break(tag: &str) -> bool {
    let name = extract_tag_name(tag);
    SUMMARY_BREAK_TAGS.contains(&name.as_str())
}

fn extract_tag_name(tag: &str) -> String {
    let normalized = tag
        .trim()
        .trim_start_matches('/')
        .trim_start_matches('!')
        .trim_start_matches('?');
    normalized
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn push_space_if_needed(output: &mut String) {
    if !output.is_empty() && !output.ends_with(char::is_whitespace) {
        output.push(' ');
    }
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trim_boundary_quotes(input: &str) -> String {
    let mut value = input.trim().to_string();
    loop {
        let next = value.trim_matches(is_boundary_quote).trim().to_string();
        if next == value {
            return next;
        }
        value = next;
    }
}

fn is_boundary_quote(character: char) -> bool {
    matches!(
        character,
        '"' | '\''
            | '`'
            | '´'
            | '«'
            | '»'
            | '“'
            | '”'
            | '„'
            | '‟'
            | '‘'
            | '’'
            | '‚'
            | '‹'
            | '›'
            | '「'
            | '」'
            | '『'
            | '』'
            | '〝'
            | '〞'
    )
}

#[cfg(test)]
mod tests {
    use super::{normalize_author_identity, split_author_value};

    #[test]
    fn split_author_value_expands_obvious_composite_names() {
        assert_eq!(
            split_author_value("Alice Martin, Bob Stone and Chloé Durand"),
            vec![
                "Alice Martin".to_string(),
                "Bob Stone".to_string(),
                "Chloé Durand".to_string(),
            ]
        );
    }

    #[test]
    fn split_author_value_discards_locations_and_credit_suffixes() {
        assert_eq!(
            split_author_value("Adélie Aubaret avec AFP, à Bastia"),
            vec!["Adélie Aubaret".to_string()]
        );
        assert_eq!(
            split_author_value("Anne Le Nir, au Liban"),
            vec!["Anne Le Nir".to_string()]
        );
        assert_eq!(
            split_author_value("Anne Le Nir au Liban"),
            vec!["Anne Le Nir".to_string()]
        );
        assert_eq!(
            split_author_value("avec Agnès Rotivel"),
            vec!["Agnès Rotivel".to_string()]
        );
        assert_eq!(
            split_author_value("Axel Chouvel avec Orthodoxie.com"),
            vec!["Axel Chouvel".to_string()]
        );
        assert_eq!(
            split_author_value("Jean Dupont, correspondant à Kiev"),
            vec!["Jean Dupont".to_string()]
        );
        assert_eq!(
            split_author_value("Jean Dupont envoyée spéciale à Gaza"),
            vec!["Jean Dupont".to_string()]
        );
        assert_eq!(
            split_author_value("Delphine Nerbollier à Berlin"),
            vec!["Delphine Nerbollier".to_string()]
        );
        assert_eq!(
            split_author_value("Delphine Nerbollier à Stuttgart"),
            vec!["Delphine Nerbollier".to_string()]
        );
        assert_eq!(
            split_author_value("Delphine Nerbollier correspondante"),
            vec!["Delphine Nerbollier".to_string()]
        );
        assert_eq!(
            split_author_value("Delphine Nerbollier (à Berlin)"),
            vec!["Delphine Nerbollier".to_string()]
        );
    }

    #[test]
    fn split_author_value_strips_editorial_prefixes() {
        assert_eq!(
            split_author_value("de notre correspondant au Liban"),
            vec!["correspondant au Liban".to_string()]
        );
        assert_eq!(
            split_author_value("propos recueillis par Jean Dupont"),
            vec!["Jean Dupont".to_string()]
        );
        assert_eq!(
            split_author_value("recueilli par J. D."),
            vec!["J. D.".to_string()]
        );
        assert_eq!(
            split_author_value("reported by AFP"),
            vec!["AFP".to_string()]
        );
        assert_eq!(
            split_author_value("Text John Doe"),
            vec!["John Doe".to_string()]
        );
        assert_eq!(
            split_author_value("edited by sarah smaje executive producers are molly glassey"),
            vec!["sarah smaje".to_string(), "molly glassey".to_string()]
        );
        assert_eq!(
            split_author_value("Olivier-Marie JOSEPH aumônier d’hôpital (diocèse de Créteil)"),
            vec!["Olivier-Marie JOSEPH".to_string()]
        );
    }

    #[test]
    fn split_author_value_keeps_non_author_conjunction_labels_whole() {
        assert_eq!(
            split_author_value("Research and Markets"),
            vec!["Research and Markets".to_string()]
        );
    }

    #[test]
    fn split_author_value_discards_generic_editorial_labels() {
        assert_eq!(
            split_author_value("A Guardian reporter"),
            Vec::<String>::new()
        );
        assert_eq!(
            split_author_value("notre correspondant régional"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn normalize_author_identity_removes_accents_and_collapses_spacing() {
        assert_eq!(
            normalize_author_identity("  Chloé   Dùrand  "),
            Some("chloe durand".to_string())
        );
    }
}
