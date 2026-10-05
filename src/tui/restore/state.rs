//! Restore/backup write-flow models; the global shell no longer owns this workflow.
#[path = "lifecycle.rs"]
mod lifecycle;
#[derive(Debug, Default)]
pub struct RestoreState {
    pub(super) wizard: Option<WizardState>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteKind {
    Restore,
    BackupCreate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardStage {
    Confirm,
    Running,
    PostRestore,
    VolumeLabelInput,
    PasswordInput,
    EncryptedFormatConfirm,
    FormatConfirm,
    Formatting,
    ReinitializePassword,
    ReinitializePasswordConfirm,
    ReinitializeConfirm,
    Reinitializing,
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostRestoreLabelTarget {
    PlainFormat,
    EncryptedFormat,
    Reinitialize,
}

pub type ExpectedIdentity = crate::application::media_identity::MediaIdentityResumePin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteIntent {
    pub kind: WriteKind,
    pub disk: u32,
    pub backup: Option<std::path::PathBuf>,
    pub expected_identity: Option<ExpectedIdentity>,
}

#[derive(Debug, Clone)]
pub struct PostRestoreFormatIntent {
    pub disk: u32,
    pub outcome: crate::application::post_restore::MetadataRestoreOutcome,
    pub request: crate::application::post_restore::PartitionFormatRequest,
    pub volume_label: String,
}

#[derive(Debug, Clone)]
pub struct EncryptedPostRestoreFormatIntent {
    pub disk: u32,
    pub outcome: crate::application::post_restore::MetadataRestoreOutcome,
    pub request: crate::application::post_restore::PartitionFormatRequest,
    pub password: Option<crate::provision::SecretBytes>,
    pub volume_label: String,
}

#[derive(Debug, Clone)]
pub struct PostRestoreReinitializeIntent {
    pub disk: u32,
    pub outcome: crate::application::post_restore::MetadataRestoreOutcome,
    pub request: crate::application::post_restore::EncryptedPartitionReinitializeRequest,
    pub filesystem: crate::filesystem::FilesystemKind,
    pub volume_label: String,
}

#[derive(Debug, Clone)]
pub struct WizardState {
    pub stage: WizardStage,
    pub kind: WriteKind,
    pub disk: u32,
    pub backup: Option<std::path::PathBuf>,
    pub expected_identity: Option<ExpectedIdentity>,
    pub confirmation: String,
    pub message: Option<crate::tui::ui::UiMessage>,
    pub restore_outcome: Option<crate::application::post_restore::MetadataRestoreOutcome>,
    pub post_restore_workbench: crate::tui::result_workbench::ResultWorkbenchState,
    pub pending_format: Option<crate::application::post_restore::PartitionFormatRequest>,
    pub volume_label_input: String,
    pub volume_label_target: Option<PostRestoreLabelTarget>,
    pub secret_input: crate::provision::SecretBytes,
    pub secret_first: crate::provision::SecretBytes,
    pub run: Option<crate::application::progress::OperationRunState>,
    pub format_run: Option<crate::application::progress::OperationRunState>,
}
