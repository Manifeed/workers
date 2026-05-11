use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::model::RssSource;

mod author_patterns;
mod author_tokens;
mod authors;
mod html;
mod media;
mod text;

use authors::extract_authors;
use html::clean_summary_text;
use media::extract_image_url;
use text::normalize_required_text;

pub fn normalize_sources(
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use chrono::{TimeZone, Utc};

    use super::normalize_sources;

    #[test]
    fn normalizes_parsed_feed_entries_without_http() {
        let feed = feed_rs::parser::parse(Cursor::new(
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <rss version="2.0">
              <channel>
                <title>Example</title>
                <item>
                  <title>  A useful title  </title>
                  <link>https://example.test/a</link>
                  <pubDate>Sun, 10 May 2026 10:00:00 GMT</pubDate>
                  <description><![CDATA[<p>Hello <strong>world</strong></p>]]></description>
                  <author>editor@example.test (Ada Lovelace)</author>
                </item>
              </channel>
            </rss>"#,
        ))
        .unwrap();

        let sources = normalize_sources(
            &feed,
            Some(Utc.with_ymd_and_hms(2026, 5, 10, 9, 0, 0).unwrap()),
        );

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].title, "A useful title");
        assert_eq!(sources[0].url, "https://example.test/a");
        assert_eq!(sources[0].summary.as_deref(), Some("Hello world"));
        assert_eq!(
            sources[0].published_at,
            Some(Utc.with_ymd_and_hms(2026, 5, 10, 10, 0, 0).unwrap())
        );
        assert!(!sources[0].authors.is_empty());
    }
}
