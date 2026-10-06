use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;

use async_lsp::{
    ClientSocket,
    lsp_types::{
        notification::{
            DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidCreateFiles,
            DidDeleteFiles, DidOpenTextDocument, DidRenameFiles, DidSaveTextDocument, Exit,
            Initialized, SetTrace,
        },
        request::{
            Completion, DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest,
            Initialize, PrepareRenameRequest, RangeFormatting, References, Rename, Shutdown,
            WorkspaceSymbolRequest,
        },
    },
    router::Router,
};
use tokio::sync::{RwLock, mpsc::UnboundedSender};
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::{config::WorkspaceProtoConfigs, log, state::ProtoLanguageState};

use notification::Notification;
use notification::worker::Worker;

mod notification;
pub(crate) mod progress;
mod request;

pub struct ProtoLanguageServer {
    pub client: ClientSocket,
    pub state: ProtoLanguageState,
    pub configs: Arc<RwLock<WorkspaceProtoConfigs>>,
    pub shutdown_token: CancellationToken,
    notification_tx: UnboundedSender<Notification>,
}

impl ProtoLanguageServer {
    pub fn new(
        client: ClientSocket,
        log_handle: log::LogReloadHandle,
        cli_include_paths: Vec<PathBuf>,
        fallback_include_path: Option<PathBuf>,
        shutdown_token: CancellationToken,
    ) -> Self {
        let configs = Arc::new(RwLock::new(WorkspaceProtoConfigs::new(
            cli_include_paths,
            fallback_include_path,
        )));
        let state = ProtoLanguageState::new();
        let notification_tx = Worker::start(
            state.clone(),
            configs.clone(),
            log_handle,
            client.clone(),
            shutdown_token.child_token(),
        );

        Self {
            notification_tx,
            state,
            configs,
            client,
            shutdown_token,
        }
    }

    pub fn new_router(
        client: ClientSocket,
        log_handle: log::LogReloadHandle,
        cli_include_paths: Vec<PathBuf>,
        fallback_include_path: Option<PathBuf>,
        shutdown_token: CancellationToken,
    ) -> Router<Self> {
        let router = Router::new(Self::new(
            client,
            log_handle,
            cli_include_paths,
            fallback_include_path,
            shutdown_token,
        ));

        // Ignore any unknown notification.
        router.unhandled_notification(|_, notif| {
            tracing::info!(notif.method, "ignored unknown notification");
            ControlFlow::Continue(())
        });

        // Handling request
        router
            .request::<Initialize, _>(Self::initialize)
            .request::<Shutdown, _>(Self::shutdown)
            .request::<HoverRequest, _>(Self::hover)
            .request::<Completion, _>(Self::completion)
            .request::<PrepareRenameRequest, _>(Self::prepare_rename)
            .request::<Rename, _>(Self::rename)
            .request::<References, _>(Self::references)
            .request::<GotoDefinition, _>(Self::definition)
            .request::<DocumentSymbolRequest, _>(Self::document_symbol)
            .request::<WorkspaceSymbolRequest, _>(Self::workspace_symbol)
            .request::<Formatting, _>(Self::formatting)
            .request::<RangeFormatting, _>(Self::range_formatting);

        // Handling notification
        router
            .notification::<Initialized>(Self::handle_notification)
            .notification::<SetTrace>(Self::handle_notification)
            .notification::<DidSaveTextDocument>(Self::handle_notification)
            .notification::<DidOpenTextDocument>(Self::handle_notification)
            .notification::<DidChangeTextDocument>(Self::handle_notification)
            .notification::<DidCloseTextDocument>(Self::handle_notification)
            .notification::<DidCreateFiles>(Self::handle_notification)
            .notification::<DidRenameFiles>(Self::handle_notification)
            .notification::<DidDeleteFiles>(Self::handle_notification)
            .notification::<DidChangeWatchedFiles>(Self::handle_notification)
            .notification::<Exit>(Self::handle_notification);

        router
    }

    #[inline]
    pub fn new_request_guard(&self) -> DropGuard {
        self.shutdown_token.child_token().drop_guard()
    }
}
