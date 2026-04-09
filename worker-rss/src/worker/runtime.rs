use std::sync::Arc;

use tokio::task::JoinSet;
use tracing::warn;

use super::scheduling::{
    idle_state, ClaimedFeedQueue, CompletedFeed, CompletedTask, ScheduledFeed,
};
use super::{FeedFetcher, Result, RssGateway};
use crate::error::RssWorkerError;
use crate::logging::stdout_log;
use crate::model::RawFeedScrapeResult;

struct CompletedTaskAck {
    task_id: u64,
}

pub struct RssWorker<G, F> {
    pub(crate) gateway: G,
    fetcher: Arc<F>,
    max_in_flight_requests: usize,
    max_in_flight_requests_per_host: usize,
    last_requested_state: Option<super::RssGatewayState>,
}

impl<G, F> RssWorker<G, F>
where
    G: RssGateway + Clone + Send + Sync + 'static,
    F: FeedFetcher + Send + Sync + 'static,
{
    pub fn new(
        gateway: G,
        fetcher: F,
        max_in_flight_requests: usize,
        max_in_flight_requests_per_host: usize,
    ) -> Self {
        Self {
            gateway,
            fetcher: Arc::new(fetcher),
            max_in_flight_requests: max_in_flight_requests.max(1),
            max_in_flight_requests_per_host: max_in_flight_requests_per_host.max(1),
            last_requested_state: None,
        }
    }

    pub async fn run_once(&mut self) -> Result<bool> {
        let mut claimed_feeds = ClaimedFeedQueue::new(self.max_in_flight_requests_per_host);
        let mut fetch_join_set = JoinSet::<CompletedFeed>::new();
        let mut completion_join_set = JoinSet::<Result<CompletedTaskAck>>::new();
        let mut last_requested_state = self.last_requested_state.clone();
        let mut pending_completion_count = 0usize;
        let mut should_claim = true;
        let mut claimed_any_tasks = false;

        loop {
            drain_completion_acks(
                &mut completion_join_set,
                &mut pending_completion_count,
                &mut should_claim,
            )?;

            if should_claim {
                let available_task_slots = self
                    .max_in_flight_requests
                    .saturating_sub(claimed_feeds.task_count() + pending_completion_count);
                if available_task_slots > 0 {
                    let claimed_tasks = self.gateway.claim(available_task_slots).await?;
                    if !claimed_tasks.is_empty() {
                        claimed_any_tasks = true;
                        for task in claimed_tasks {
                            stdout_log(format!("claim task {} received", task.task_id));
                            claimed_feeds.enqueue_task(task);
                        }
                        request_state(
                            &mut last_requested_state,
                            &self.gateway,
                            claimed_feeds.processing_state(pending_completion_count),
                        )
                        .await;
                    }
                }
                should_claim = false;
            }

            while fetch_join_set.len() < self.max_in_flight_requests {
                let Some(scheduled_feed) = claimed_feeds.try_start_next() else {
                    break;
                };
                spawn_fetch(
                    &mut fetch_join_set,
                    Arc::clone(&self.fetcher),
                    scheduled_feed,
                );
            }
            if claimed_feeds.has_tasks()
                || claimed_feeds.has_pending_or_active_feeds()
                || pending_completion_count > 0
            {
                request_state(
                    &mut last_requested_state,
                    &self.gateway,
                    claimed_feeds.processing_state(pending_completion_count),
                )
                .await;
            }

            if fetch_join_set.is_empty() {
                if !claimed_feeds.has_tasks() {
                    if pending_completion_count == 0 {
                        if claimed_any_tasks {
                            self.last_requested_state = last_requested_state;
                            return Ok(true);
                        }
                        request_state(&mut last_requested_state, &self.gateway, idle_state()).await;
                        self.last_requested_state = last_requested_state;
                        return Ok(claimed_any_tasks);
                    }
                    wait_for_completion_ack(
                        &mut completion_join_set,
                        &mut pending_completion_count,
                        &mut should_claim,
                    )
                    .await?;
                    continue;
                }

                if !claimed_feeds.has_pending_or_active_feeds() {
                    return Err(RssWorkerError::Runtime(
                        "worker has claimed rss tasks but no feed is queued for execution"
                            .to_string(),
                    ));
                }
            }

            let worker_event = tokio::select! {
                Some(joined) = fetch_join_set.join_next() => WorkerEvent::Feed(
                    joined.map_err(|error| map_join_error(error, "rss fetch join failed"))?
                ),
                Some(joined) = completion_join_set.join_next(), if !completion_join_set.is_empty() => {
                    WorkerEvent::CompletionAck(joined)
                }
            };

            match worker_event {
                WorkerEvent::Feed(completed_feed) => {
                    if let Some(completed_task) = claimed_feeds.finish_feed(completed_feed)? {
                        spawn_completion_ack(
                            &mut completion_join_set,
                            self.gateway.clone(),
                            completed_task,
                        );
                        pending_completion_count += 1;
                        should_claim = true;
                    }
                }
                WorkerEvent::CompletionAck(joined) => {
                    finish_completion_ack(
                        joined,
                        &mut pending_completion_count,
                        &mut should_claim,
                    )?;
                }
            }
        }
    }
}

enum WorkerEvent {
    Feed(CompletedFeed),
    CompletionAck(std::result::Result<Result<CompletedTaskAck>, tokio::task::JoinError>),
}

fn spawn_fetch<F>(
    fetch_join_set: &mut JoinSet<CompletedFeed>,
    fetcher: Arc<F>,
    scheduled_feed: ScheduledFeed,
) where
    F: FeedFetcher + Send + Sync + 'static,
{
    fetch_join_set.spawn(async move {
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
    });
}

fn spawn_completion_ack<G>(
    completion_join_set: &mut JoinSet<Result<CompletedTaskAck>>,
    gateway: G,
    completed_task: CompletedTask,
) where
    G: RssGateway + Clone + Send + Sync + 'static,
{
    completion_join_set.spawn(async move {
        gateway
            .complete(
                completed_task.task_key.task_id,
                completed_task.task_key.execution_id,
                completed_task.results,
            )
            .await?;
        Ok(CompletedTaskAck {
            task_id: completed_task.task_key.task_id,
        })
    });
}

fn drain_completion_acks(
    completion_join_set: &mut JoinSet<Result<CompletedTaskAck>>,
    pending_completion_count: &mut usize,
    should_claim: &mut bool,
) -> Result<()> {
    while let Some(joined) = completion_join_set.try_join_next() {
        finish_completion_ack(joined, pending_completion_count, should_claim)?;
    }
    Ok(())
}

async fn wait_for_completion_ack(
    completion_join_set: &mut JoinSet<Result<CompletedTaskAck>>,
    pending_completion_count: &mut usize,
    should_claim: &mut bool,
) -> Result<()> {
    let Some(joined) = completion_join_set.join_next().await else {
        return Err(RssWorkerError::Runtime(
            "worker is waiting for task completion acknowledgements but none are running"
                .to_string(),
        ));
    };
    finish_completion_ack(joined, pending_completion_count, should_claim)
}

fn finish_completion_ack(
    joined: std::result::Result<Result<CompletedTaskAck>, tokio::task::JoinError>,
    pending_completion_count: &mut usize,
    should_claim: &mut bool,
) -> Result<()> {
    *pending_completion_count = pending_completion_count.saturating_sub(1);
    let acknowledged_task =
        joined.map_err(|error| map_join_error(error, "rss completion join failed"))??;
    stdout_log(format!("return {}", acknowledged_task.task_id));
    *should_claim = true;
    Ok(())
}

async fn request_state<G>(
    last_requested_state: &mut Option<super::RssGatewayState>,
    gateway: &G,
    state: super::RssGatewayState,
) where
    G: RssGateway + Clone + Send + Sync + 'static,
{
    if matches_reporting_state(last_requested_state.as_ref(), &state) {
        return;
    }
    *last_requested_state = Some(state.clone());
    if let Err(error) = gateway.update_state(state).await {
        warn!("rss worker state update failed: {error}");
    }
}

fn map_join_error(error: tokio::task::JoinError, context: &str) -> RssWorkerError {
    RssWorkerError::Runtime(format!("{context}: {error}"))
}

fn matches_reporting_state(
    previous_state: Option<&super::RssGatewayState>,
    state: &super::RssGatewayState,
) -> bool {
    previous_state.is_some_and(|previous_state| previous_state.is_equivalent_for_reporting(state))
}
