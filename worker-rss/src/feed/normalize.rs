use std::collections::HashSet;

use chrono::{DateTime, Utc};
use feed_rs::model::Entry;

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
            author: entry.authors.first().map(|author| author.name.clone()),
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
    use std::io::Cursor;

    use feed_rs::parser;

    use super::normalize_sources;

    #[test]
    fn normalize_sources_skips_entries_with_blank_titles_or_links_after_trim() {
        let rss = r#"
            <rss version="2.0">
              <channel>
                <title>Example</title>
                <item>
                  <title>   </title>
                  <link>https://example.com/blank-title</link>
                  <pubDate>Thu, 19 Mar 2026 12:00:00 GMT</pubDate>
                </item>
                <item>
                  <title>Blank link</title>
                  <link>   </link>
                  <pubDate>Thu, 19 Mar 2026 12:01:00 GMT</pubDate>
                </item>
                <item>
                  <title>  Valid title  </title>
                  <link>  https://example.com/valid  </link>
                  <description>Summary</description>
                  <pubDate>Thu, 19 Mar 2026 12:02:00 GMT</pubDate>
                </item>
              </channel>
            </rss>
        "#;
        let feed = parser::parse(Cursor::new(rss.as_bytes())).expect("feed should parse");

        let sources = normalize_sources(&feed, None);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].title, "Valid title");
        assert_eq!(sources[0].url, "https://example.com/valid");
        assert_eq!(sources[0].summary.as_deref(), Some("Summary"));
    }
}
