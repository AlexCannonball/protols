use std::path::PathBuf;

use async_lsp::ResponseError;
use async_lsp::lsp_types::{
    CompletionOptions, DefinitionOptions, FileOperationFilter, FileOperationPattern,
    FileOperationPatternKind, FileOperationRegistrationOptions, HoverOptions,
    HoverProviderCapability, InitializeParams, InitializeResult, OneOf, RenameOptions,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    WorkDoneProgressOptions, WorkspaceFileOperationsServerCapabilities,
    WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities, WorkspaceSymbolOptions,
};
use futures::FutureExt;
use futures::future::BoxFuture;
use serde_json::Value;

use crate::ProtoLanguageServer;

impl ProtoLanguageServer {
    pub(in crate::server) fn initialize(
        &mut self,
        params: InitializeParams,
    ) -> BoxFuture<'static, Result<InitializeResult, ResponseError>> {
        log_client_info(&params);

        let configs = self.configs.clone();
        let workspace = Some(build_workspace_capabilities(&params));
        let definition_provider = build_definition_provider(&params);
        let hover_provider = build_hover_provider(&params);
        let workspace_symbol_provider = build_workspace_symbol_provider(&params);
        let rename_provider = build_rename_provider(&params);

        async move {
            if let Some(init_options) = &params.initialization_options
                && let Some(include_paths) = parse_init_include_paths(init_options)
            {
                tracing::info!(
                    "Setting include paths from initialization options: {:?}",
                    include_paths
                );
                configs.write().await.set_init_include_paths(include_paths);
            }

            if let Some(folders) = &params.workspace_folders
                && !folders.is_empty()
            {
                let mut config = configs.write().await;
                for workspace in folders {
                    tracing::info!("Workspace folder: {:?}", workspace);
                    config.add_workspace_folder(workspace).await;
                }
            } else {
                tracing::info!("Running in no workspace mode");
                configs.write().await.no_workspace_mode();
            }

            let response = InitializeResult {
                capabilities: ServerCapabilities {
                    // todo(): We might prefer incremental sync at some later stage
                    text_document_sync: Some(TextDocumentSyncCapability::Kind(
                        TextDocumentSyncKind::FULL,
                    )),
                    workspace,
                    definition_provider,
                    hover_provider,
                    document_symbol_provider: Some(OneOf::Left(true)),
                    workspace_symbol_provider,
                    completion_provider: Some(CompletionOptions::default()),
                    rename_provider,
                    document_formatting_provider: Some(OneOf::Left(true)),
                    document_range_formatting_provider: Some(OneOf::Left(true)),
                    references_provider: Some(OneOf::Left(true)),
                    ..ServerCapabilities::default()
                },
                server_info: Some(ServerInfo {
                    name: env!("CARGO_PKG_NAME").to_string(),
                    version: Some(env!("CARGO_PKG_VERSION").to_string()),
                }),
            };

            Ok(response)
        }
        .boxed()
    }

    async fn init_include_paths(&mut self, params: &InitializeParams) {
        if let Some(init_options) = &params.initialization_options
            && let Some(include_paths) = parse_init_include_paths(init_options)
        {
            tracing::info!(
                "Setting include paths from initialization options: {:?}",
                include_paths
            );
            self.configs
                .write()
                .await
                .set_init_include_paths(include_paths);
        }
    }
}

/// Parse `include_paths` from initialization options
#[inline]
fn parse_init_include_paths(init_options: &Value) -> Option<Vec<PathBuf>> {
    let mut result = vec![];
    let paths = init_options["include_paths"].as_array()?;

    for path_value in paths {
        if let Some(path) = path_value.as_str() {
            result.push(PathBuf::from(path));
        } else {
            tracing::warn!(
                "Invalid include path in initialization options: {:?}",
                path_value
            );
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[inline]
fn log_client_info(params: &InitializeParams) {
    let name = params
        .client_info
        .as_ref()
        .map_or("<unknown>", |c| c.name.as_str());
    let version = params
        .client_info
        .as_ref()
        .and_then(|c| c.version.as_deref())
        .unwrap_or("<unknown>");

    tracing::info!("Connected with client {name} {version}");
}

#[inline]
fn build_workspace_capabilities(params: &InitializeParams) -> WorkspaceServerCapabilities {
    let file_operations = build_file_operations(params);

    WorkspaceServerCapabilities {
        workspace_folders: Some(WorkspaceFoldersServerCapabilities {
            supported: Some(true),
            change_notifications: Some(OneOf::Left(
                params
                    .capabilities
                    .workspace
                    .as_ref()
                    .and_then(|w| w.workspace_folders)
                    .unwrap_or_default(),
            )),
        }),
        file_operations,
    }
}

#[inline]
fn build_file_operations(
    params: &InitializeParams,
) -> Option<WorkspaceFileOperationsServerCapabilities> {
    let file_operations = params
        .capabilities
        .workspace
        .as_ref()?
        .file_operations
        .as_ref()?;

    let did_create = file_operations.did_create.unwrap_or_default();
    let did_delete = file_operations.did_delete.unwrap_or_default();
    let did_rename = file_operations.did_rename.unwrap_or_default();

    Some(WorkspaceFileOperationsServerCapabilities {
        did_create: did_create.then_some(build_file_operation_filters()),
        did_delete: did_delete.then_some(build_file_operation_filters()),
        did_rename: did_rename.then_some(build_file_operation_filters()),
        ..Default::default()
    })
}

#[inline]
fn build_file_operation_filters() -> FileOperationRegistrationOptions {
    FileOperationRegistrationOptions {
        filters: vec![FileOperationFilter {
            scheme: Some(String::from("file")),
            pattern: FileOperationPattern {
                glob: String::from("**/*.proto"),
                matches: Some(FileOperationPatternKind::File),
                ..Default::default()
            },
        }],
    }
}

#[inline]
fn build_definition_provider(params: &InitializeParams) -> Option<OneOf<bool, DefinitionOptions>> {
    params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|cap| cap.definition.as_ref())
        .map(|_| {
            OneOf::Right(DefinitionOptions {
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: Some(false),
                },
            })
        })
}

#[inline]
fn build_hover_provider(params: &InitializeParams) -> Option<HoverProviderCapability> {
    params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|cap| cap.hover.as_ref())
        .map(|_| {
            HoverProviderCapability::Options(HoverOptions {
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: Some(false),
                },
            })
        })
}

#[inline]
fn build_workspace_symbol_provider(
    params: &InitializeParams,
) -> Option<OneOf<bool, WorkspaceSymbolOptions>> {
    let work_done_progress_options = build_work_done_progress_options(params);

    params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|cap| cap.symbol.as_ref())
        .map(|_| {
            OneOf::Right(WorkspaceSymbolOptions {
                resolve_provider: Some(false),
                work_done_progress_options,
            })
        })
}

#[inline]
fn build_rename_provider(params: &InitializeParams) -> Option<OneOf<bool, RenameOptions>> {
    let work_done_progress_options = build_work_done_progress_options(params);

    params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|cap| cap.rename.as_ref())
        .map(|rc| {
            OneOf::Right(RenameOptions {
                prepare_provider: rc.prepare_support,
                work_done_progress_options,
            })
        })
}

#[inline]
fn build_work_done_progress_options(params: &InitializeParams) -> WorkDoneProgressOptions {
    let work_done_progress = params
        .capabilities
        .window
        .as_ref()
        .and_then(|w| w.work_done_progress);

    WorkDoneProgressOptions { work_done_progress }
}
