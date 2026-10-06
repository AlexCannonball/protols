use async_lsp::lsp_types::notification::Exit;

use super::{Notification, Worker};

impl Worker {
    pub(super) fn exit(&mut self) {
        if self.shutdown_token.is_cancelled() {
            tracing::info!(
                "Received exit notification after shutdown. Terminating background worker cleanly."
            );
        } else {
            tracing::warn!(
                "Received exit notification WITHOUT shutdown! Process state is inconsistent."
            );
        }
    }
}

impl From<Exit> for Notification {
    fn from(_params: Exit) -> Self {
        Notification::Exit
    }
}
