use crate::metrics::{StageMetrics, SyncMetrics};
use reth_primitives::{
    stage::{StageCheckpoint, StageId},
    BlockNumber,
};
use std::{
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// Alias type for metric producers to use.
pub type MetricEventsSender = UnboundedSender<MetricEvent>;

/// Collection of metric events.
#[derive(Clone, Copy, Debug)]
pub enum MetricEvent {
    /// Stage reached new checkpoint.
    StageCheckpoint {
        /// Stage ID.
        stage_id: StageId,
        /// Stage checkpoint.
        checkpoint: StageCheckpoint,
        /// Maximum known block number reachable by this stage.
        /// If specified, `entities_total` metric is updated.
        max_block_number: Option<BlockNumber>,
    },
}

/// Metrics routine that listens to new metric events on the `events_rx` receiver.
/// Upon receiving new event, related metrics are updated.
#[derive(Debug)]
pub struct MetricsListener {
    events_rx: UnboundedReceiver<MetricEvent>,
    pub(crate) sync_metrics: SyncMetrics,
}

impl MetricsListener {
    /// Creates a new [MetricsListener] with the provided receiver of [MetricEvent].
    pub fn new(events_rx: UnboundedReceiver<MetricEvent>) -> Self {
        Self { events_rx, sync_metrics: SyncMetrics::default() }
    }

    fn handle_event(&mut self, event: MetricEvent) {
        match event {
            MetricEvent::StageCheckpoint { stage_id, checkpoint, max_block_number } => {
                let stage_metrics = self.sync_metrics.stages.entry(stage_id).or_insert_with(|| {
                    StageMetrics::new_with_labels(&[("stage", stage_id.to_string())])
                });

                stage_metrics.checkpoint.set(checkpoint.block_number as f64);

                let (processed, total) = match checkpoint.entities() {
                    Some(entities) => (entities.processed, Some(entities.total)),
                    None => (checkpoint.block_number, max_block_number),
                };

                stage_metrics.entities_processed.set(processed as f64);

                if let Some(total) = total {
                    stage_metrics.entities_total.set(total as f64);
                }
            }
        }
    }
}

impl Future for MetricsListener {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        // Loop until we drain the `events_rx` channel
        loop {
            let Some(event) = ready!(this.events_rx.poll_recv(cx)) else {
                // Channel has closed
                return Poll::Ready(())
            };

            this.handle_event(event);
        }
    }
}