pub(super) fn normalize_required_text(value: &str) -> Option<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return None;
    }
    Some(normalized.to_string())
}

pub(super) fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn trim_boundary_quotes(input: &str) -> String {
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
    use super::{normalize_required_text, trim_boundary_quotes};

    #[test]
    fn normalize_required_text_rejects_blank_values() {
        assert_eq!(
            normalize_required_text("  hello  "),
            Some("hello".to_string())
        );
        assert_eq!(normalize_required_text("   "), None);
    }

    #[test]
    fn trim_boundary_quotes_strips_nested_quotes() {
        assert_eq!(
            trim_boundary_quotes(" \"“Bonjour”\" "),
            "Bonjour".to_string()
        );
    }
}
