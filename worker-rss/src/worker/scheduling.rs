use std::collections::{BTreeMap, VecDeque};

use crate::error::{Result, RssWorkerError};
use crate::feed::scheduling_host_key;
use crate::model::{ClaimedRssTask, RawFeedScrapeResult, RssFeedPayload};

use super::RssGatewayState;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct TaskKey {
    pub(crate) task_id: u64,
    pub(crate) execution_id: u64,
}

struct ScheduledTask {
    job_id: String,
    total_feeds: usize,
    remaining_feeds: usize,
    results: Vec<Option<RawFeedScrapeResult>>,
}

impl ScheduledTask {
    fn new(task: &ClaimedRssTask) -> Self {
        Self {
            job_id: task.job_id.clone(),
            total_feeds: task.feeds.len(),
            remaining_feeds: task.feeds.len(),
            results: vec![None; task.feeds.len()],
        }
    }

    fn record_result(&mut self, feed_index: usize, result: RawFeedScrapeResult) -> Result<()> {
        let Some(slot) = self.results.get_mut(feed_index) else {
            return Err(RssWorkerError::Runtime(format!(
                "missing rss result slot for feed index {feed_index}"
            )));
        };
        if slot.is_some() {
            return Err(RssWorkerError::Runtime(format!(
                "rss result for feed index {feed_index} was completed twice"
            )));
        }
        *slot = Some(result);
        self.remaining_feeds = self.remaining_feeds.saturating_sub(1);
        Ok(())
    }

    fn is_complete(&self) -> bool {
        self.remaining_feeds == 0
    }

    fn label(&self) -> String {
        let completed_feeds = self.total_feeds.saturating_sub(self.remaining_feeds);
        format!(
            "job {} [{} / {} feeds]",
            self.job_id, completed_feeds, self.total_feeds
        )
    }

    fn into_results(self) -> Result<Vec<RawFeedScrapeResult>> {
        self.results
            .into_iter()
            .enumerate()
            .map(|(feed_index, result)| {
                result.ok_or_else(|| {
                    RssWorkerError::Runtime(format!(
                        "missing RSS result for feed index {feed_index}"
                    ))
                })
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ScheduledFeed {
    pub(crate) host_key: String,
    pub(crate) task_key: TaskKey,
    pub(crate) feed_index: usize,
    pub(crate) job_id: String,
    pub(crate) ingest: bool,
    pub(crate) feed: RssFeedPayload,
}

pub(crate) struct CompletedFeed {
    pub(crate) scheduled_feed: ScheduledFeed,
    pub(crate) result: RawFeedScrapeResult,
}

pub(crate) struct CompletedTask {
    pub(crate) task_key: TaskKey,
    pub(crate) results: Vec<RawFeedScrapeResult>,
}

pub(crate) struct ClaimedFeedQueue {
    tasks: BTreeMap<TaskKey, ScheduledTask>,
    pending_by_host: BTreeMap<String, VecDeque<ScheduledFeed>>,
    active_by_host: BTreeMap<String, Vec<ScheduledFeed>>,
    max_active_per_host: usize,
}

impl ClaimedFeedQueue {
    pub(crate) fn new(max_active_per_host: usize) -> Self {
        Self {
            tasks: BTreeMap::new(),
            pending_by_host: BTreeMap::new(),
            active_by_host: BTreeMap::new(),
            max_active_per_host: max_active_per_host.max(1),
        }
    }

    pub(crate) fn task_count(&self) -> usize {
        self.tasks.len()
    }

    pub(crate) fn has_tasks(&self) -> bool {
        !self.tasks.is_empty()
    }

    pub(crate) fn has_pending_or_active_feeds(&self) -> bool {
        !self.pending_by_host.is_empty() || !self.active_by_host.is_empty()
    }

    pub(crate) fn enqueue_task(&mut self, task: ClaimedRssTask) {
        let task_key = TaskKey {
            task_id: task.task_id,
            execution_id: task.execution_id,
        };

        for (feed_index, feed) in task.feeds.iter().cloned().enumerate() {
            let host_key = scheduling_host_key(&feed);
            self.pending_by_host
                .entry(host_key.clone())
                .or_default()
                .push_back(ScheduledFeed {
                    host_key,
                    task_key,
                    feed_index,
                    job_id: task.job_id.clone(),
                    ingest: task.ingest,
                    feed,
                });
        }

        self.tasks.insert(task_key, ScheduledTask::new(&task));
    }

    pub(crate) fn try_start_next(&mut self) -> Option<ScheduledFeed> {
        let host_key = self
            .pending_by_host
            .iter()
            .find(|(host_key, feeds)| {
                !feeds.is_empty()
                    && self.active_by_host.get(*host_key).map_or(0, Vec::len)
                        < self.max_active_per_host
            })
            .map(|(host_key, _)| host_key.clone())?;

        let mut pending_feeds = self.pending_by_host.remove(&host_key)?;
        let scheduled_feed = pending_feeds.pop_front()?;
        if !pending_feeds.is_empty() {
            self.pending_by_host.insert(host_key.clone(), pending_feeds);
        }

        self.active_by_host
            .entry(host_key)
            .or_default()
            .push(scheduled_feed.clone());
        Some(scheduled_feed)
    }

    pub(crate) fn finish_feed(
        &mut self,
        completed_feed: CompletedFeed,
    ) -> Result<Option<CompletedTask>> {
        self.remove_active_feed(&completed_feed.scheduled_feed)?;

        let task = self
            .tasks
            .get_mut(&completed_feed.scheduled_feed.task_key)
            .ok_or_else(|| {
                RssWorkerError::Runtime("missing task progress for completed feed".to_string())
            })?;
        task.record_result(
            completed_feed.scheduled_feed.feed_index,
            completed_feed.result,
        )?;
        if !task.is_complete() {
            return Ok(None);
        }

        let task = self
            .tasks
            .remove(&completed_feed.scheduled_feed.task_key)
            .ok_or_else(|| {
                RssWorkerError::Runtime("missing completed task state".to_string())
            })?;
        Ok(Some(CompletedTask {
            task_key: completed_feed.scheduled_feed.task_key,
            results: task.into_results()?,
        }))
    }

    pub(crate) fn processing_state(&self, pending_completion_count: usize) -> RssGatewayState {
        let feeds_claimed: usize = self.tasks.values().map(|task| task.total_feeds).sum();
        let queued_feeds: usize = self.pending_by_host.values().map(VecDeque::len).sum();
        let first_task = self.tasks.iter().next().map(|(task_key, _)| *task_key);
        let first_task_label = self
            .tasks
            .iter()
            .next()
            .map(|(_, task)| task.label())
            .or_else(|| {
                (pending_completion_count > 0)
                    .then(|| format!("committing {pending_completion_count} completed rss task(s)"))
            });
        let current_feed = self
            .active_by_host
            .values()
            .find_map(|feeds| feeds.first())
            .or_else(|| self.pending_by_host.values().find_map(|feeds| feeds.front()));
        let connection_state = if pending_completion_count > 0 && self.active_by_host.is_empty() {
            "committing"
        } else {
            "processing"
        };

        RssGatewayState {
            active: true,
            connection_state: connection_state.to_string(),
            pending_tasks: (self.tasks.len() + pending_completion_count) as u32,
            current_task_id: first_task.map(|task_key| task_key.task_id),
            current_execution_id: first_task.map(|task_key| task_key.execution_id),
            current_task_label: first_task_label.map(|label| {
                format!(
                    "{label} | {} rss task(s), {} feeds, {} queued",
                    self.tasks.len(),
                    feeds_claimed,
                    queued_feeds
                )
            }),
            current_feed_id: current_feed.map(|feed| feed.feed.feed_id),
            current_feed_url: current_feed.map(|feed| feed.feed.feed_url.clone()),
            desired_state: Some("running".to_string()),
            ..Default::default()
        }
    }

    fn remove_active_feed(&mut self, scheduled_feed: &ScheduledFeed) -> Result<()> {
        let active_feeds = self
            .active_by_host
            .get_mut(&scheduled_feed.host_key)
            .ok_or_else(|| {
                RssWorkerError::Runtime("missing active host entry for completed feed".to_string())
            })?;
        let Some(active_feed_index) = active_feeds.iter().position(|active_feed| {
            active_feed.task_key == scheduled_feed.task_key
                && active_feed.feed_index == scheduled_feed.feed_index
        }) else {
            return Err(RssWorkerError::Runtime(
                "missing active feed entry for completed feed".to_string(),
            ));
        };

        active_feeds.swap_remove(active_feed_index);
        if active_feeds.is_empty() {
            self.active_by_host.remove(&scheduled_feed.host_key);
        }
        Ok(())
    }
}

pub(crate) fn idle_state() -> RssGatewayState {
    RssGatewayState {
        active: true,
        connection_state: "idle".to_string(),
        pending_tasks: 0,
        desired_state: Some("running".to_string()),
        ..Default::default()
    }
}
