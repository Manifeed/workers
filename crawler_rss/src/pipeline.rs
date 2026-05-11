use crate::model::RawFeedScrapeResult;
use crate::ports::FeedFetcher;
use crate::runtime_scheduling::{CompletedFeed, ScheduledFeed};

pub(crate) async fn execute_feed_micro_task<F>(
    fetcher: &F,
    scheduled_feed: ScheduledFeed,
) -> CompletedFeed
where
    F: FeedFetcher + Send + Sync,
{
    let result = match fetcher
        .fetch(
            &scheduled_feed.job_id,
            scheduled_feed.ingest,
            &scheduled_feed.feed,
        )
        .await
    {
        Ok(result) => result,
        Err(error) => RawFeedScrapeResult::error(
            scheduled_feed.job_id.as_str(),
            scheduled_feed.ingest,
            &scheduled_feed.feed,
            None,
            Some(scheduled_feed.feed.fetchprotection),
            error.to_string(),
        ),
    };
    CompletedFeed {
        scheduled_feed,
        result,
    }
}
