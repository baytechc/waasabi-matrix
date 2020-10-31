use once_cell::sync::{Lazy, OnceCell};
use std::sync::RwLock;

use tokio::runtime::Runtime;
use super::{DispatchGuard, Dispatcher};

static GLOBAL_DISPATCHER: Lazy<RwLock<Dispatcher>> =
    Lazy::new(|| RwLock::new(Dispatcher::new()));

fn guard() -> &'static DispatchGuard {
    static GLOBAL_GUARD: OnceCell<DispatchGuard> = OnceCell::new();

    GLOBAL_GUARD.get_or_init(|| {
        let lock = GLOBAL_DISPATCHER.read().unwrap();
        lock.guard()
    })
}

/// Launches a new task on the global dispatch queue.
///
/// The new task will be enqueued immediately.
/// If the pre-init queue was already flushed,
/// the background thread will process tasks in the queue (see [`flush_init`]).
///
/// This will not block.
///
/// [`flush_init`]: fn.flush_init.html
pub fn launch(task: impl FnOnce(&mut Runtime) + Send + 'static) {
    match guard().launch(task) {
        Ok(_) => {}
        Err(_) => {
            log::info!("Failed to launch a task on the queue. Discarding task.");
        }
    }
}
