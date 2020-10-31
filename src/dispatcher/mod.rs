use std::thread::{self, JoinHandle};

use crossbeam_channel::{unbounded, SendError, Sender, TrySendError};
use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;
use tokio::runtime::Runtime;

pub use global::*;

mod global;

/// The command a worker should execute.
enum Command {
    /// A task is a user-defined function to run.
    Task(Box<dyn FnOnce(&mut Runtime) + Send>),
}

/// The error returned from operations on the dispatcher
#[derive(Debug, PartialEq)]
pub enum DispatchError {
    /// Failed to send command to worker thread
    SendError,
}

impl From<TrySendError<Command>> for DispatchError {
    fn from(_: TrySendError<Command>) -> Self {
        DispatchError::SendError
    }
}

impl<T> From<SendError<T>> for DispatchError {
    fn from(_: SendError<T>) -> Self {
        DispatchError::SendError
    }
}

/// A clonable guard for a dispatch queue.
#[derive(Clone)]
struct DispatchGuard {
    /// Sender for the unbounded queue.
    sender: Sender<Command>,
}

impl DispatchGuard {
    pub fn launch(
        &self,
        task: impl FnOnce(&mut Runtime) + Send + 'static,
    ) -> Result<(), DispatchError> {
        let task = Command::Task(Box::new(task));
        self.send(task)
    }

    fn send(&self, task: Command) -> Result<(), DispatchError> {
        self.sender.send(task)?;
        Ok(())
    }
}

/// A dispatcher.
///
/// Run expensive processing tasks sequentially off the main thread.
/// Tasks are processed in a single separate thread in the order they are submitted.
/// The dispatch queue will enqueue tasks while not flushed, up to the maximum queue size.
/// Processing will start after flushing once, processing already enqueued tasks first, then
/// waiting for further tasks to be enqueued.
pub struct Dispatcher {
    /// Guard used for communication with the worker thread.
    guard: DispatchGuard,

    /// Handle to the worker thread, allows to wait for it to finish.
    #[allow(dead_code)]
    worker: Option<JoinHandle<()>>,
}

impl Dispatcher {
    /// Creates a new dispatcher with a maximum queue size.
    ///
    /// Launched tasks won't run until [`flush_init`] is called.
    ///
    /// [`flush_init`]: #method.flush_init
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();

        let worker = thread::spawn(move || {
            let quota = Quota::per_minute(NonZeroU32::new(60).unwrap());
            let limiter = RateLimiter::direct(quota);
            let mut rt = Runtime::new().unwrap();

            loop {
                use Command::*;

                rt.block_on(async {
                    limiter.until_ready().await;
                });

                match receiver.recv() {
                    Ok(Task(f)) => {
                        (f)(&mut rt);
                    }

                    // Other side was disconnected.
                    Err(_) => {
                        log::error!("The task producer was disconnected. Worker thread will exit.");
                        return;
                    }
                }
            }
        });

        let guard = DispatchGuard { sender };

        Dispatcher {
            guard,
            worker: Some(worker),
        }
    }

    fn guard(&self) -> DispatchGuard {
        self.guard.clone()
    }
}
