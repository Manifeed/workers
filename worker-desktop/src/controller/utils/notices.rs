use manifeed_worker_common::{user_facing_error_message, WorkerError};

pub(crate) fn connection_failure_notice(status_code: Option<u16>, detail: Option<&str>) -> String {
    match status_code {
        Some(401 | 403) => "Invalid API key".to_string(),
        Some(404) => "Invalid API URL".to_string(),
        Some(408 | 504) => "Request timeout".to_string(),
        Some(500..=599) => "Backend unavailable".to_string(),
        _ => super::compact_status_detail(detail, "Request failed"),
    }
}

pub(crate) fn connection_error_notice(error: &WorkerError) -> String {
    user_facing_error_message(error)
}

pub(crate) fn summarize_detail(detail: &str) -> String {
    sanitized_optional_detail(Some(detail)).unwrap_or_else(|| "Please try again.".to_string())
}

pub(crate) fn sanitized_optional_detail(detail: Option<&str>) -> Option<String> {
    let detail = detail?;
    let first_line = detail.lines().next().unwrap_or_default().trim();
    if first_line.is_empty() {
        return None;
    }

    let collapsed = first_line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }

    let mut summary = collapsed;
    if summary.len() > 140 {
        summary.truncate(137);
        summary.push_str("...");
    }
    Some(summary)
}

#[cfg(test)]
mod tests {
    use super::connection_failure_notice;

    #[test]
    fn connection_failures_map_common_http_statuses() {
        assert_eq!(
            connection_failure_notice(Some(403), None),
            "Invalid API key"
        );
        assert_eq!(
            connection_failure_notice(Some(404), None),
            "Invalid API URL"
        );
    }
}
