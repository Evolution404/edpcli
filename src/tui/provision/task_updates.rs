//! Provision result routing; TaskHub only dispatches the feature envelope.
use super::*;
impl TaskHub {
    pub(crate) fn retain_password_session(&mut self, session: Option<u64>) {
        if self.provision.password_session == session {
            return;
        }
        self.provision.password_session = session;
        for slot in &mut self.provision.password_verify_slots {
            slot.invalidate_pending();
        }
    }

    pub(super) fn route_provision_result(
        &mut self,
        result: ProvisionWorkerResult,
        updates: &mut ProvisionUpdates,
    ) {
        match result {
            ProvisionWorkerResult::KeyProbe {
                generation,
                context,
                result,
            } => match self.provision.key_probe_slot.finish_latest(generation) {
                LatestCompletion::Restart {
                    generation,
                    request,
                } => {
                    self.start_key_probe(generation, request);
                }
                LatestCompletion::Deliver(true)
                    if self.provision.key_probe_context == Some(context) =>
                {
                    updates.key_probe =
                        Some((context, result.map_err(|error| error.in_phase("制盘"))));
                }
                LatestCompletion::Deliver(_) => {}
            },
            ProvisionWorkerResult::KeyVerify {
                generation,
                session_id,
                revision,
                domain,
                result,
            } => {
                match self.provision.password_verify_slots[password_domain_index(domain)]
                    .finish_latest(generation)
                {
                    LatestCompletion::Restart {
                        generation,
                        request,
                    } => self.start_source_password_verify(generation, request),
                    LatestCompletion::Deliver(true) => updates.key_verify.push((
                        session_id,
                        domain,
                        revision,
                        result.map_err(|error| error.in_phase("来源密码验证")),
                    )),
                    LatestCompletion::Deliver(false) => {}
                }
            }
            ProvisionWorkerResult::Plan { generation, result } => {
                if self.provision.plan_slot.finish(generation) {
                    updates.plan = Some(result.map_err(|error| error.in_phase("制盘")));
                }
            }
            ProvisionWorkerResult::Progress {
                operation_id,
                event,
            } => {
                if self.active_operation == Some(operation_id) {
                    crate::tui::progress_transport::push_progress_coalesced(
                        &mut updates.progress,
                        operation_id,
                        event,
                    );
                }
            }
            ProvisionWorkerResult::Write {
                operation_id,
                result,
            } => {
                if self.finish_operation(operation_id) {
                    updates.write = Some((
                        operation_id,
                        result.map_err(|error| error.in_phase("制盘写入")),
                    ));
                }
            }
            ProvisionWorkerResult::Export { generation, result } => {
                if self.provision.export_slot.finish(generation) {
                    updates.export = Some(result.map_err(|error| error.in_phase("制盘")));
                }
            }
        }
    }
}
