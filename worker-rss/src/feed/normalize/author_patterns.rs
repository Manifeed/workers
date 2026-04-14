use std::sync::OnceLock;

use regex::Regex;

pub(super) fn author_list_split_regex() -> &'static Regex {
    static AUTHOR_LIST_SPLIT_REGEX: OnceLock<Regex> = OnceLock::new();
    AUTHOR_LIST_SPLIT_REGEX
        .get_or_init(|| Regex::new(r"\s*[;,]\s*").expect("author list split regex must be valid"))
}

pub(super) fn author_conjunction_split_regex() -> &'static Regex {
    static AUTHOR_CONJUNCTION_SPLIT_REGEX: OnceLock<Regex> = OnceLock::new();
    AUTHOR_CONJUNCTION_SPLIT_REGEX.get_or_init(|| {
        Regex::new(r"(?i)\s*(?:&|\band\b|\bet\b)\s*")
            .expect("author conjunction split regex must be valid")
    })
}

pub(super) fn leading_byline_regex() -> &'static Regex {
    static LEADING_BYLINE_REGEX: OnceLock<Regex> = OnceLock::new();
    LEADING_BYLINE_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(?:(?:par|by)\s+)+").expect("leading byline regex must be valid")
    })
}

pub(super) fn inline_editorial_separator_regexes() -> [&'static Regex; 3] {
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

pub(super) fn leading_editorial_prefix_regexes() -> [&'static Regex; 6] {
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

pub(super) fn leading_with_regex() -> &'static Regex {
    static LEADING_WITH_REGEX: OnceLock<Regex> = OnceLock::new();
    LEADING_WITH_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(?:avec|with)\s+(.+)$").expect("leading with regex must be valid")
    })
}

pub(super) fn trailing_with_regex() -> &'static Regex {
    static TRAILING_WITH_REGEX: OnceLock<Regex> = OnceLock::new();
    TRAILING_WITH_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(.+?)\s+(?:avec|with)\s+(.+)$")
            .expect("trailing with regex must be valid")
    })
}

pub(super) fn domain_fragment_regex() -> &'static Regex {
    static DOMAIN_FRAGMENT_REGEX: OnceLock<Regex> = OnceLock::new();
    DOMAIN_FRAGMENT_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(?:^|[\s(])(?:www\.)?[a-z0-9-]+\.(?:com|fr|org|net|io|co|info|tv|fm|be|ch|de|uk|eu)(?:$|[\s)])",
        )
        .expect("domain fragment regex must be valid")
    })
}

pub(super) fn role_label_regex() -> &'static Regex {
    static ROLE_LABEL_REGEX: OnceLock<Regex> = OnceLock::new();
    ROLE_LABEL_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:correspondance|correspondant(?:e)?|correspondent|special correspondent|envoy(?:e|é)e?\s+sp(?:e|é)cial(?:e)?)\b",
        )
        .expect("role label regex must be valid")
    })
}

pub(super) fn trailing_role_regex() -> &'static Regex {
    static TRAILING_ROLE_REGEX: OnceLock<Regex> = OnceLock::new();
    TRAILING_ROLE_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^(.+?)(?:\s*[,/-]\s*|\s+)(?:correspondance|correspondant(?:e)?|correspondent|special correspondent|envoy(?:e|é)e?\s+sp(?:e|é)cial(?:e)?)\b.*$",
        )
        .expect("trailing role regex must be valid")
    })
}

pub(super) fn trailing_location_regex() -> &'static Regex {
    static TRAILING_LOCATION_REGEX: OnceLock<Regex> = OnceLock::new();
    TRAILING_LOCATION_REGEX.get_or_init(|| {
        Regex::new(r"(?i)^(.+?)(?:\s*[,/-]\s*|\s+)(?:in|at|a|à|au|aux|en|dans|depuis|sur)\s+.+$")
            .expect("trailing location regex must be valid")
    })
}

pub(super) fn generic_editorial_label_regex() -> &'static Regex {
    static GENERIC_EDITORIAL_LABEL_REGEX: OnceLock<Regex> = OnceLock::new();
    GENERIC_EDITORIAL_LABEL_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:a|an|the|un|une|le|la|les)\s+.+\b(?:reporter|editor|staff|team|desk|bureau|newsroom|redaction|rédaction|editorial)\b.*$",
        )
        .expect("generic editorial label regex must be valid")
    })
}

pub(super) fn role_location_regex() -> &'static Regex {
    static ROLE_LOCATION_REGEX: OnceLock<Regex> = OnceLock::new();
    ROLE_LOCATION_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:correspondance|correspondant(?:e)?|correspondent|special correspondent|envoy(?:e|é)e?\s+sp(?:e|é)cial(?:e)?)\s+(?:in|at|a|à|au|aux|en|dans|depuis|sur)\s+.+$",
        )
        .expect("role location regex must be valid")
    })
}

pub(super) fn name_word_regex() -> &'static Regex {
    static NAME_WORD_REGEX: OnceLock<Regex> = OnceLock::new();
    NAME_WORD_REGEX.get_or_init(|| {
        Regex::new(r"^[A-Za-zÀ-ÖØ-öø-ÿ]+(?:[-'’][A-Za-zÀ-ÖØ-öø-ÿ]+)*$")
            .expect("name word regex must be valid")
    })
}

pub(super) fn initial_token_regex() -> &'static Regex {
    static INITIAL_TOKEN_REGEX: OnceLock<Regex> = OnceLock::new();
    INITIAL_TOKEN_REGEX.get_or_init(|| {
        Regex::new(r"^(?:[A-Za-zÀ-ÖØ-öø-ÿ]\.)+(?:-[A-Za-zÀ-ÖØ-öø-ÿ]\.)*$")
            .expect("initial token regex must be valid")
    })
}

pub(super) fn parenthetical_chunk_regex() -> &'static Regex {
    static PARENTHETICAL_CHUNK_REGEX: OnceLock<Regex> = OnceLock::new();
    PARENTHETICAL_CHUNK_REGEX.get_or_init(|| {
        Regex::new(r"\s*\([^()]*\)").expect("parenthetical chunk regex must be valid")
    })
}
