use chrono::{DateTime, Utc};

use crate::model::RssSource;

pub fn drop_future_published_sources(
    sources: Vec<RssSource>,
    now: DateTime<Utc>,
) -> Vec<RssSource> {
    sources
        .into_iter()
        .filter(|source| {
            source
                .published_at
                .map(|published_at| published_at <= now)
                .unwrap_or(true)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone, Utc};

    use super::drop_future_published_sources;
    use crate::model::RssSource;

    fn source(url: &str, published_at: Option<DateTime<Utc>>) -> RssSource {
        RssSource {
            title: url.to_string(),
            url: format!("https://example.test/{url}"),
            summary: None,
            authors: Vec::new(),
            published_at,
            image_url: None,
        }
    }

    #[test]
    fn drops_sources_published_after_now() {
        let now = Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap();
        let kept_past = source(
            "past",
            Some(Utc.with_ymd_and_hms(2026, 5, 10, 11, 59, 59).unwrap()),
        );
        let kept_now = source("now", Some(now));
        let kept_without_date = source("no-date", None);
        let future = source(
            "future",
            Some(Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 1).unwrap()),
        );

        let filtered = drop_future_published_sources(
            vec![
                kept_past.clone(),
                future,
                kept_now.clone(),
                kept_without_date.clone(),
            ],
            now,
        );

        assert_eq!(filtered, vec![kept_past, kept_now, kept_without_date]);
    }
}
