use std::sync::Arc;
use tokio::sync::watch;

pub struct Handle {
    pub(crate) shutdown_tx: Arc<watch::Sender<bool>>,
    pub(crate) join_handle: tokio::task::JoinHandle<()>,
}

impl Handle {
    pub async fn shutdown(self) {
        self.shutdown_tx.send(true).unwrap();
        self.join_handle.await.unwrap();
    }
}
