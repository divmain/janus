//! Generic async action queue for TUI screens

use crate::tui::components::Toast;
use iocraft::prelude::*;
use std::pin::Pin;
use std::sync::Arc;
use std::future::Future;
use tokio::sync::mpsc;

const MAX_BATCH_SIZE: usize = 10;

/// Generic action for the queue
pub trait Action: Send + Clone + 'static {
    /// Execute the action
    fn execute(self) -> Pin<Box<dyn Future<Output = ActionResult> + Send>>;
}

/// Result of an action execution
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub success: bool,
    pub message: Option<String>,
}

/// Channel for sending actions
#[derive(Clone)]
pub struct ActionChannel<A>
where
    A: Send + Clone + 'static,
{
    pub tx: mpsc::UnboundedSender<A>,
    _rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<A>>>,
}

impl<A: Send + Clone> ActionChannel<A> {
    pub fn send(&self, action: A) -> Result<(), mpsc::error::SendError<A>> {
        self.tx.send(action)
    }
}

/// Action queue state
pub struct ActionQueueState<A, P>
where
    A: Send + Clone + 'static,
{
    _channel: ActionChannel<A>,
    _processor: P,
    _is_processing: bool,
}

/// Builder for creating an action queue
pub struct ActionQueueBuilder<A, P> {
    _phantom: std::marker::PhantomData<(A, P)>,
}

impl<A, P> ActionQueueBuilder<A, P>
where
    A: Send + Clone + 'static,
    P: Send + Sync + 'static,
{
    /// Create a new action queue with state
    pub fn use_state(
        hooks: &mut Hooks,
        action_processor: P,
        needs_reload: State<bool>,
        toast: State<Option<Toast>>,
    ) -> (
        ActionQueueState<A, P>,
        Handler<()>,
        ActionChannel<A>,
    )
    where
        P: Fn(Vec<A>, State<bool>, State<Option<Toast>>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Clone + 'static,
    {
        let channel: State<ActionChannel<A>> = hooks.use_state(|| {
            let (tx, rx) = mpsc::unbounded_channel::<A>();
            ActionChannel {
                tx,
                _rx: Arc::new(tokio::sync::Mutex::new(rx)),
            }
        });

        let action_processor_clone = action_processor.clone();
        let needs_reload_clone = needs_reload.clone();
        let toast_clone = toast.clone();

        let channel_state_for_handler = channel.clone();
        let action_handler: Handler<()> = hooks.use_async_handler({
            let action_processor_clone = action_processor_clone.clone();
            let needs_reload = needs_reload_clone.clone();
            let toast = toast_clone.clone();
            let channel_state = channel_state_for_handler.clone();

            move |_| Box::pin({
                let action_processor = action_processor_clone.clone();
                let needs_reload = needs_reload.clone();
                let toast = toast.clone();

                async move {
                    let mut actions = Vec::new();

                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                        let channel_ref = channel_state.read();
                        if let Ok(mut rx) = channel_ref._rx.try_lock() {
                            while let Ok(action) = rx.try_recv() {
                                actions.push(action);
                                if actions.len() >= MAX_BATCH_SIZE {
                                    break;
                                }
                            }
                        }

                        if !actions.is_empty() {
                            let actions_to_process = std::mem::take(&mut actions);
                            action_processor(
                                actions_to_process,
                                needs_reload.clone(),
                                toast.clone(),
                            ).await;
                        }
                    }
                }
            })
        });

        let channel_inner = channel.read();
        let tx = channel_inner.tx.clone();
        let channel_clone = ActionChannel {
            tx: tx.clone(),
            _rx: Arc::new(tokio::sync::Mutex::new(mpsc::unbounded_channel::<A>().1)),
        };

        let queue_state = ActionQueueState {
            _channel: channel_clone.clone(),
            _processor: action_processor,
            _is_processing: true,
        };

        (queue_state, action_handler, channel_clone)
    }
}
