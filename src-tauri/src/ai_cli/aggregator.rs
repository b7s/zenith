//! Pure service: aggregate per-CLI session states into the three-dot booleans.
//! Single source of truth for the dot-precedence logic.

use std::collections::HashMap;

use super::model::{AggregateState, CliEvent, CliEventType, CliId, CliSnapshot, UnseenFailure};

/// Internal tracker — holds all known session state for each CLI.
#[derive(Debug, Default)]
pub struct Aggregator {
    running: HashMap<CliId, Vec<String>>,
    unseen_failures: Vec<UnseenFailure>,
}

impl Aggregator {
    /// Ingest one event from a CLI signal source.
    pub fn ingest(&mut self, event: CliEvent) {
        match event.event_type {
            CliEventType::Started => {
                let label = event.prompt_label.unwrap_or_else(|| "working".into());
                self.running.entry(event.cli_id).or_default().push(label);
            }
            CliEventType::Idle | CliEventType::Completed => {
                if let Some(labels) = self.running.get_mut(&event.cli_id) {
                    if !labels.is_empty() {
                        labels.pop();
                    }
                }
            }
            CliEventType::Failed => {
                if let Some(labels) = self.running.get_mut(&event.cli_id) {
                    labels.pop();
                }
                self.unseen_failures.push(UnseenFailure {
                    cli_id: event.cli_id,
                    error_message: event.error_message.unwrap_or_else(|| "Unknown error".into()),
                    occurred_at: event.timestamp_ms,
                    prompt_label: event.prompt_label.unwrap_or_default(),
                });
            }
        }
    }

    /// Mark ALL unseen failures as seen (user opened the manager window).
    pub fn ack_all(&mut self) {
        self.unseen_failures.clear();
    }

    /// Compute the aggregate three-boolean state for the bar widget dot.
    pub fn aggregate(&self) -> AggregateState {
        let has_unseen = !self.unseen_failures.is_empty();
        let any_running = self.running.values().any(|l| !l.is_empty());
        let any_finished = self
            .running
            .values()
            .any(|l| l.is_empty())
            || (!has_unseen && !any_running);

        AggregateState {
            has_unseen_failure: has_unseen,
            any_running,
            any_finished,
            per_cli: self.snapshots(),
        }
    }

    fn snapshots(&self) -> Vec<CliSnapshot> {
        let mut ids: Vec<CliId> = self
            .running
            .keys()
            .chain(self.unseen_failures.iter().map(|f| &f.cli_id))
            .cloned()
            .collect();
        ids.sort();
        ids.dedup();

        ids
            .into_iter()
            .map(|id| {
                let is_running = self.running.get(&id).map_or(false, |l| !l.is_empty());
                let last_err = self
                    .unseen_failures
                    .iter()
                    .rev()
                    .find(|f| f.cli_id == id);
                let prompt_label = self
                    .running
                    .get(&id)
                    .and_then(|l| l.last())
                    .cloned()
                    .unwrap_or_default();
                let status_text = if is_running {
                    format!("running: {prompt_label}")
                } else if last_err.is_some() {
                    "failed".into()
                } else {
                    "idle".into()
                };
                CliSnapshot {
                    cli_id: id.as_str().into(),
                    is_running,
                    last_error_at: last_err.map(|f| f.occurred_at),
                    last_error_message: last_err
                        .map(|f| f.error_message.clone())
                        .unwrap_or_default(),
                    current_prompt_label: prompt_label,
                    status_text,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(cli: CliId, ty: CliEventType) -> CliEvent {
        CliEvent {
            cli_id: cli,
            event_type: ty,
            prompt_label: None,
            error_message: None,
            timestamp_ms: 0,
        }
    }

    fn event_fail(cli: CliId, msg: &str) -> CliEvent {
        CliEvent {
            cli_id: cli,
            event_type: CliEventType::Failed,
            prompt_label: None,
            error_message: Some(msg.into()),
            timestamp_ms: 0,
        }
    }

    #[test]
    fn idle_gives_green() {
        let mut a = Aggregator::default();
        let state = a.aggregate();
        assert!(!state.has_unseen_failure);
        assert!(!state.any_running);
        assert!(state.any_finished);
    }

    #[test]
    fn running_gives_blue() {
        let mut a = Aggregator::default();
        a.ingest(event(CliId::Opencode, CliEventType::Started));
        let state = a.aggregate();
        assert!(!state.has_unseen_failure);
        assert!(state.any_running);
        assert!(!state.any_finished);
    }

    #[test]
    fn failure_gives_red() {
        let mut a = Aggregator::default();
        a.ingest(event_fail(CliId::Claude, "API timeout"));
        let state = a.aggregate();
        assert!(state.has_unseen_failure);
        assert!(!state.any_running);
        assert!(!state.any_finished);
    }

    #[test]
    fn failure_running_both_shows_red_and_blue() {
        let mut a = Aggregator::default();
        a.ingest(event(CliId::Opencode, CliEventType::Started));
        a.ingest(event_fail(CliId::Codex, "crash"));
        let state = a.aggregate();
        assert!(state.has_unseen_failure);
        assert!(state.any_running);
        assert!(!state.any_finished);
    }

    #[test]
    fn ack_clears_red() {
        let mut a = Aggregator::default();
        a.ingest(event_fail(CliId::Claude, "timeout"));
        assert!(a.aggregate().has_unseen_failure);
        a.ack_all();
        assert!(!a.aggregate().has_unseen_failure);
    }

    #[test]
    fn started_then_completed_is_green() {
        let mut a = Aggregator::default();
        a.ingest(event(CliId::Opencode, CliEventType::Started));
        a.ingest(event(CliId::Opencode, CliEventType::Completed));
        let state = a.aggregate();
        assert!(!state.has_unseen_failure);
        assert!(!state.any_running);
        assert!(state.any_finished);
    }

    #[test]
    fn failure_keeps_running_independent() {
        let mut a = Aggregator::default();
        a.ingest(event(CliId::Opencode, CliEventType::Started));
        a.ingest(event(CliId::Opencode, CliEventType::Started));
        a.ingest(event_fail(CliId::Codex, "err"));
        let state = a.aggregate();
        assert!(state.has_unseen_failure);
        assert!(state.any_running);
        assert!(!state.any_finished);
        // Ack failure does not affect running
        a.ack_all();
        let state = a.aggregate();
        assert!(!state.has_unseen_failure);
        assert!(state.any_running);
    }
}
