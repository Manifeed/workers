use serde::Serialize;

pub use crate::ports::{FeedFetcher, RssGateway, TaskGateway};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct RssGatewayState {
    pub active: bool,
    pub connection_state: String,
    pub pending_tasks: u32,
    pub current_task_id: Option<u64>,
    pub current_execution_id: Option<u64>,
    pub current_task_label: Option<String>,
    pub current_feed_id: Option<u64>,
    pub current_feed_url: Option<String>,
    pub last_error: Option<String>,
    pub desired_state: Option<String>,
}

impl RssGatewayState {
    pub fn is_equivalent_for_reporting(&self, other: &Self) -> bool {
        self.active == other.active
            && self.connection_state == other.connection_state
            && self.pending_tasks == other.pending_tasks
            && self.current_task_id == other.current_task_id
            && self.current_execution_id == other.current_execution_id
            && self.last_error == other.last_error
            && self.desired_state == other.desired_state
    }
}

#[cfg(test)]
mod tests {
    use super::RssGatewayState;

    #[test]
    fn reporting_equivalence_ignores_volatile_feed_progress_fields() {
        let base = RssGatewayState {
            active: true,
            connection_state: "processing".to_string(),
            pending_tasks: 2,
            current_task_id: Some(21),
            current_execution_id: Some(21),
            current_task_label: Some("job a [1 / 10 feeds]".to_string()),
            current_feed_id: Some(11),
            current_feed_url: Some("https://example.com/feed-a.xml".to_string()),
            last_error: None,
            desired_state: Some("running".to_string()),
        };
        let changed_progress = RssGatewayState {
            current_task_label: Some("job a [9 / 10 feeds]".to_string()),
            current_feed_id: Some(19),
            current_feed_url: Some("https://example.com/feed-b.xml".to_string()),
            ..base.clone()
        };

        assert!(base.is_equivalent_for_reporting(&changed_progress));
    }
}
