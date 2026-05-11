use feed_rs::model::Entry;

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

#[cfg(test)]
mod tests {
    use super::find_first_image_src;

    #[test]
    fn find_first_image_src_reads_html_entities() {
        assert_eq!(
            find_first_image_src(
                r#"<div><img src="https://cdn.example.com/photo&amp;size=large.jpg" /></div>"#
            ),
            Some("https://cdn.example.com/photo&size=large.jpg".to_string())
        );
    }
}
