use async_lsp::lsp_types::notification::{DidChangeWatchedFiles, Notification as N};
use async_lsp::lsp_types::request::RegisterCapability;
use async_lsp::lsp_types::{
    DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, InitializedParams, Registration,
    RegistrationParams, WatchKind,
};

use super::{Notification, Worker};

impl Worker {
    pub(super) async fn initialized(&mut self, _params: InitializedParams) {
        let paths = self.configs.read().await.get_all_indexing_paths();

        if paths.is_empty() {
            tracing::info!("No indexing paths found. Skipping initial workspace warmup.");
            return;
        } else {
            self.run_indexing_pipeline(paths).await;
        }

        let client_clone = self.client.clone();

        tokio::spawn(async move {
            let watcher_options = DidChangeWatchedFilesRegistrationOptions {
                watchers: vec![FileSystemWatcher {
                    glob_pattern: async_lsp::lsp_types::GlobPattern::String(String::from(
                        "**/*.proto",
                    )),
                    kind: Some(WatchKind::Create | WatchKind::Change | WatchKind::Delete),
                }],
            };

            let register_options = serde_json::to_value(watcher_options).unwrap_or_default();

            let registration_params = RegistrationParams {
                registrations: vec![Registration {
                    id: String::from("protols-proto-file-watcher"),
                    method: DidChangeWatchedFiles::METHOD.to_string(),
                    register_options: Some(register_options),
                }],
            };

            if let Err(error) = client_clone
                .request::<RegisterCapability>(registration_params)
                .await
            {
                tracing::error!(
                    ?error,
                    "Client failed to register dynamic file watcher capability for **/*.proto files. External disk changes might not sync properly."
                );
            } else {
                tracing::info!(
                    "Dynamic file watcher for **/*.proto files successfully registered on the client side."
                );
            }
        });
    }
}

impl From<InitializedParams> for Notification {
    fn from(params: InitializedParams) -> Self {
        Notification::Initialized { params }
    }
}
