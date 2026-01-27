//! Generic async action queue for TUI screens

use crate::tui::components::Toast;
use iocraft::prelude::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

const CHANNEL_CAPACITY: usize = 100;
const MAX_BATCH_SIZE: usize = 10;

/// Result of an action execution
#[derive(Debug, Clone)]
pub enum ActionResult {
    /// Simple success/error result
    Result {
        success: bool,
        message: Option<String>,
    },
    /// LoadForEdit result with ticket data
    LoadForEdit {
        success: bool,
        message: Option<String>,
        id: String,
        metadata: Box<TicketMetadata>,
        body: String,
    },
}

impl ActionResult {
    pub fn success(&self) -> bool {
        match self {
            ActionResult::Result { success, .. } => *success,
            ActionResult::LoadForEdit { success, .. } => *success,
        }
    }

    pub fn message(&self) -> Option<String> {
        match self {
            ActionResult::Result { message, .. } => message.clone(),
            ActionResult::LoadForEdit { message, .. } => message.clone(),
        }
    }
}

/// Ticket metadata for load actions (simplified version, avoids full import)
#[derive(Debug, Clone, Default)]
pub struct TicketMetadata {
    pub id: Option<String>,
    pub uuid: Option<String>,
    pub title: Option<String>,
    pub status: Option<crate::types::TicketStatus>,
    pub ticket_type: Option<crate::types::TicketType>,
    pub priority: Option<crate::types::TicketPriority>,
    pub triaged: Option<bool>,
    pub created: Option<String>,
    pub file_path: Option<String>,
    pub deps: Vec<String>,
    pub links: Vec<String>,
    pub external_ref: Option<String>,
    pub remote: Option<String>,
    pub parent: Option<String>,
    pub spawned_from: Option<String>,
    pub spawn_context: Option<String>,
    pub depth: Option<u32>,
    pub completion_summary: Option<String>,
}

/// Channel for sending actions (bounded for backpressure)
#[derive(Clone)]
pub struct ActionChannel<A>
where
    A: Send + Clone + 'static,
{
    pub tx: tokio::sync::mpsc::Sender<A>,
}

impl<A: Send + Clone> ActionChannel<A> {
    pub fn send(&self, action: A) -> Result<(), tokio::sync::mpsc::error::SendError<A>> {
        self.tx.try_send(action).map_err(|e| match e {
            tokio::sync::mpsc::error::TrySendError::Full(_) => {
                panic!("Action queue is full - this should not happen");
            }
            tokio::sync::mpsc::error::TrySendError::Closed(action) => {
                tokio::sync::mpsc::error::SendError(action)
            }
        })
    }
}

/// Action queue state
pub struct ActionQueueState<A, P>
where
    A: Send + Clone + 'static,
{
    _channel: ActionChannel<A>,
    _processor: P,
}

/// Processor function type for action queues
pub type ActionProcessor<A> = Arc<
    dyn Fn(Vec<A>, State<bool>, State<Option<Toast>>) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

/// Builder for creating an action queue
pub struct ActionQueueBuilder<A, P> {
    _phantom: std::marker::PhantomData<(A, P)>,
}

impl<A, P> ActionQueueBuilder<A, P>
where
    A: Send + Clone + 'static,
{
    /// Create a new action queue with state
    pub fn use_state(
        hooks: &mut Hooks,
        action_processor: P,
        needs_reload: State<bool>,
        toast: State<Option<Toast>>,
    ) -> (ActionQueueState<A, P>, Handler<()>, ActionChannel<A>)
    where
        P: Fn(
                Vec<A>,
                State<bool>,
                State<Option<Toast>>,
            ) -> Pin<Box<dyn Future<Output = ()> + Send>>
            + Clone
            + 'static,
        P: Send + Sync + 'static,
    {
        let (tx, rx) = tokio::sync::mpsc::channel::<A>(CHANNEL_CAPACITY);
        let rx = Arc::new(Mutex::new(rx));

        let channel = ActionChannel { tx: tx.clone() };

        let action_processor_clone = action_processor.clone();
        let needs_reload_clone = needs_reload;
        let toast_clone = toast;

        let action_handler: Handler<()> = hooks.use_async_handler({
            let rx = rx.clone();
            move |_| {
                Box::pin({
                    let action_processor = action_processor_clone.clone();
                    let needs_reload = needs_reload_clone;
                    let toast = toast_clone;
                    let rx = rx.clone();

                    async move {
                        let mut actions = Vec::new();

                        loop {
                            tokio::select! {
                                action = async {
                                    let mut rx_guard = rx.lock().await;
                                    rx_guard.recv().await
                                } => {
                                    if let Some(action) = action {
                                        actions.push(action);

                                        let mut rx_guard = rx.lock().await;
                                        while actions.len() < MAX_BATCH_SIZE {
                                            match rx_guard.try_recv() {
                                                Ok(more_action) => actions.push(more_action),
                                                Err(_) => break,
                                            }
                                        }
                                    } else {
                                        break;
                                    }
                                }
                            }

                            if !actions.is_empty() {
                                let actions_to_process = std::mem::take(&mut actions);
                                action_processor(actions_to_process, needs_reload, toast).await;
                            }
                        }
                    }
                })
            }
        });

        let queue_state = ActionQueueState {
            _channel: channel.clone(),
            _processor: action_processor,
        };

        (queue_state, action_handler, channel)
    }
}
