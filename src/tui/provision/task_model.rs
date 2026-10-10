//! Provision-owned events, read-task slots and update batch.
use super::*;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyProbeContext {
    pub disk: u32,
    pub session_id: u64,
}
#[derive(Debug)]
pub(super) struct PasswordVerifyRequest {
    pub(super) disk: u32,
    pub(super) domain: crate::provision::KeyDomainRole,
    pub(super) password: crate::domain::secret::SecretText,
    pub(super) revision: u64,
    pub(super) session_id: u64,
}
pub(super) fn password_domain_index(domain: crate::provision::KeyDomainRole) -> usize {
    match domain {
        crate::provision::KeyDomainRole::Share => 0,
        crate::provision::KeyDomainRole::Encrypt => 1,
    }
}

pub(super) enum ProvisionWorkerResult {
    KeyProbe {
        generation: u64,
        context: KeyProbeContext,
        result: Result<
            crate::application::provision::ProvisionKeyProbe,
            crate::application::error::OperationError,
        >,
    },
    KeyVerify {
        generation: u64,
        session_id: u64,
        revision: u64,
        domain: crate::provision::KeyDomainRole,
        result: Result<
            crate::provision::SourcePasswordKnowledge,
            crate::application::error::OperationError,
        >,
    },
    Plan {
        generation: u64,
        result: Result<
            crate::application::provision::PreparedProvision,
            crate::application::error::OperationError,
        >,
    },
    NativeReadOnlyPlan {
        generation: u64,
        result: Result<
            crate::application::provision::native_preflight::Native4knReadOnlyPreflight,
            String,
        >,
    },
    Progress {
        operation_id: OperationId,
        event: crate::application::progress::ProgressEvent,
    },
    Write {
        operation_id: OperationId,
        result: Result<
            crate::application::provision::ProvisionWriteOutcome,
            crate::application::error::OperationError,
        >,
    },
    Export {
        generation: u64,
        result: Result<PathBuf, crate::application::error::OperationError>,
    },
}
#[derive(Default)]
pub struct ProvisionUpdates {
    pub key_probe: Option<(
        KeyProbeContext,
        Result<
            crate::application::provision::ProvisionKeyProbe,
            crate::application::error::OperationError,
        >,
    )>,
    pub key_verify: Vec<(
        u64,
        crate::provision::KeyDomainRole,
        u64,
        Result<
            crate::provision::SourcePasswordKnowledge,
            crate::application::error::OperationError,
        >,
    )>,
    pub plan: Option<
        Result<
            crate::application::provision::PreparedProvision,
            crate::application::error::OperationError,
        >,
    >,
    pub native_readonly_plan: Option<
        Result<crate::application::provision::native_preflight::Native4knReadOnlyPreflight, String>,
    >,
    pub progress: Vec<(OperationId, crate::application::progress::ProgressEvent)>,
    pub write: Option<(
        OperationId,
        Result<
            crate::application::provision::ProvisionWriteOutcome,
            crate::application::error::OperationError,
        >,
    )>,
    pub export: Option<Result<PathBuf, crate::application::error::OperationError>>,
}
impl ProvisionUpdates {
    pub fn has_updates(&self) -> bool {
        self.key_probe.is_some()
            || !self.key_verify.is_empty()
            || self.plan.is_some()
            || self.native_readonly_plan.is_some()
            || !self.progress.is_empty()
            || self.write.is_some()
            || self.export.is_some()
    }
}
#[derive(Default)]
pub(super) struct ProvisionTaskState {
    pub(super) key_probe_slot: TaskSlot<KeyProbeContext>,
    pub(super) key_probe_context: Option<KeyProbeContext>,
    pub(super) password_verify_slots: [TaskSlot<PasswordVerifyRequest>; 2],
    pub(super) password_session: Option<u64>,
    pub(super) plan_slot: TaskSlot<()>,
    pub(super) export_slot: TaskSlot<()>,
}
