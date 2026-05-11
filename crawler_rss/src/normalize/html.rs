use super::text::{collapse_whitespace, trim_boundary_quotes};

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

pub(super) fn strip_html_tags(input: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::clean_summary_text;

    #[test]
    fn clean_summary_text_decodes_entities_and_strips_tags() {
        assert_eq!(
            clean_summary_text("<p>&quot;Bonjour&nbsp;<strong>tout le monde</strong>&quot;</p>"),
            Some("Bonjour tout le monde".to_string())
        );
    }
}
