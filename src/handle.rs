use crate::Stats;
use event_listener::Event;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

#[derive(Debug, Clone)]
pub struct Handle {
    shutdown: Arc<Event>,
    join_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    max_conns: usize,
    conn_semaphore: Arc<Semaphore>,
}

impl Handle {
    pub(crate) fn new(
        shutdown: Arc<Event>,
        join_handle: tokio::task::JoinHandle<()>,
        max_conns: usize,
        conn_semaphore: Arc<Semaphore>,
    ) -> Self {
        Self {
            shutdown,
            join_handle: Arc::new(Mutex::new(Some(join_handle))),
            max_conns,
            conn_semaphore,
        }
    }

    pub fn stats(&self) -> Stats {
        let max_conns = self.max_conns;
        let curr_conns = max_conns - self.conn_semaphore.available_permits();
        Stats {
            curr_conns,
            max_conns,
        }
    }

    pub async fn shutdown(&self) {
        self.shutdown.notify(usize::MAX);
        let join_handle = self.join_handle.lock().unwrap().take();
        if let Some(join_handle) = join_handle {
            join_handle.await.unwrap();
        }
    }
}
