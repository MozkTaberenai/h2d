use std::sync::{Arc, Mutex};
use tokio::sync::{watch, Semaphore};

#[derive(Debug, Clone)]
pub struct Handle {
    shutdown_tx: Arc<watch::Sender<bool>>,
    join_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    max_conns: usize,
    conn_semaphore: Arc<Semaphore>,
}

impl Handle {
    pub(crate) fn new(
        shutdown_tx: Arc<watch::Sender<bool>>,
        join_handle: tokio::task::JoinHandle<()>,
        max_conns: usize,
        conn_semaphore: Arc<Semaphore>,
    ) -> Self {
        Self {
            shutdown_tx,
            join_handle: Arc::new(Mutex::new(Some(join_handle))),
            max_conns,
            conn_semaphore,
        }
    }

    pub fn current_conns(&self) -> usize {
        self.max_conns - self.conn_semaphore.available_permits()
    }

    pub async fn shutdown(&self) {
        self.shutdown_tx.send(true).unwrap();
        let join_handle = self.join_handle.lock().unwrap().take();
        if let Some(join_handle) = join_handle {
            join_handle.await.unwrap();
        }
    }
}
