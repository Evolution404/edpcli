use super::*;

impl AppState {
    pub(super) fn provision_sync_cursor_to_end(&mut self) {
        self.provision.field_cursor = self
            .provision_selected_field()
            .map(|value| value.chars().count())
            .unwrap_or(0);
    }

    pub fn provision_selected_field_is_editable(&self) -> bool {
        self.provision_field_descriptor(self.provision.field_selected)
            .is_some_and(|descriptor| descriptor.capabilities.editable)
            && self.provision_selected_field().is_some()
    }

    pub fn provision_field_cursor(&self) -> usize {
        let len = self
            .provision_selected_field()
            .map(|value| value.chars().count())
            .unwrap_or(0);
        self.provision.field_cursor.min(len)
    }

    pub fn provision_move_cursor(&mut self, delta: isize) {
        let Some(value) = self.provision_selected_field() else {
            return;
        };
        let len = value.chars().count();
        self.provision.field_cursor = if delta < 0 {
            self.provision
                .field_cursor
                .saturating_sub(delta.unsigned_abs())
        } else {
            (self.provision.field_cursor + delta as usize).min(len)
        };
    }

    pub fn provision_cursor_home(&mut self) {
        if self.provision_selected_field().is_some() {
            self.provision.field_cursor = 0;
        }
    }

    pub fn provision_cursor_end(&mut self) {
        self.provision_sync_cursor_to_end();
    }

    pub(super) fn provision_input_policy(&self, id: ProvisionFieldId) -> ProvisionInputPolicy {
        if let ProvisionFieldId::Plain { partition, kind } = id {
            return match kind {
                PlainProvisionFieldKind::StartLba => ProvisionInputPolicy::UnsignedInteger,
                PlainProvisionFieldKind::Capacity => self
                    .provision
                    .plain_form
                    .partitions
                    .get(partition)
                    .map(|part| {
                        if part.input_mode == crate::provision::CapacityInputMode::Exact {
                            ProvisionInputPolicy::UnsignedInteger
                        } else {
                            ProvisionInputPolicy::DecimalCapacity
                        }
                    })
                    .unwrap_or(ProvisionInputPolicy::UnsignedInteger),
                PlainProvisionFieldKind::Filesystem | PlainProvisionFieldKind::VolumeLabel => {
                    ProvisionInputPolicy::Text
                }
            };
        }
        match id {
            ProvisionFieldId::Capacity(crate::provision::PartitionRole::Boot) => {
                if self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            ProvisionFieldId::Capacity(
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined,
            ) => {
                if self.provision.form.share_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            ProvisionFieldId::Capacity(crate::provision::PartitionRole::Encrypt) => {
                if self.provision.form.encrypt_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            ProvisionFieldId::LabelId => ProvisionInputPolicy::OnlyId,
            ProvisionFieldId::StartLba(_) => ProvisionInputPolicy::UnsignedInteger,
            ProvisionFieldId::MaxPasswordErrors(_) => ProvisionInputPolicy::U8,
            _ => ProvisionInputPolicy::Text,
        }
    }

    pub(super) fn provision_mark_capacity_edit(&mut self, id: Option<ProvisionFieldId>) {
        let role = id.and_then(|id| match id {
            ProvisionFieldId::Capacity(role) => Some(role),
            _ => None,
        });
        self.provision.form.mark_quick_capacity_edit(role);
        if let Some(ProvisionFieldId::Plain {
            partition,
            kind: PlainProvisionFieldKind::Capacity,
        }) = id
        {
            if let Some(part) = self.provision.plain_form.partitions.get_mut(partition) {
                if part.input_mode == crate::provision::CapacityInputMode::Quick {
                    part.capacity_edited = true;
                }
            }
        }
    }

    pub fn provision_push_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let cursor = self.provision_field_cursor();
        let Some(id) = self.provision_field_id(self.provision.field_selected) else {
            return;
        };
        let Some(current) = self.provision_selected_field() else {
            return;
        };
        if current.chars().count() >= 128 {
            return;
        }
        let mut chars = current.chars().collect::<Vec<_>>();
        chars.insert(cursor.min(chars.len()), ch);
        let candidate = chars.into_iter().collect::<String>();
        let policy = self.provision_input_policy(id);
        if !policy.accepts(&candidate) {
            self.provision.message = Some(policy.rejection_message().into());
            return;
        }
        if let Some(field) = self.provision_selected_field_mut() {
            *field = candidate;
            self.provision.field_cursor = cursor + 1;
            self.provision_mark_capacity_edit(Some(id));
            self.provision_mark_source_password_unverified(Some(id));
            self.provision.message = None;
        }
    }

    pub fn provision_backspace(&mut self) {
        let cursor = self.provision_field_cursor();
        let id = self.provision_field_id(self.provision.field_selected);
        if cursor == 0 {
            return;
        }
        if let Some(field) = self.provision_selected_field_mut() {
            let mut chars = field.chars().collect::<Vec<_>>();
            if cursor <= chars.len() {
                chars.remove(cursor - 1);
                *field = chars.into_iter().collect();
                self.provision.field_cursor = cursor - 1;
                self.provision_mark_capacity_edit(id);
                self.provision_mark_source_password_unverified(id);
                self.provision.message = None;
            }
        }
    }

    pub fn provision_delete_char(&mut self) {
        let cursor = self.provision_field_cursor();
        let id = self.provision_field_id(self.provision.field_selected);
        if let Some(field) = self.provision_selected_field_mut() {
            let mut chars = field.chars().collect::<Vec<_>>();
            if cursor < chars.len() {
                chars.remove(cursor);
                *field = chars.into_iter().collect();
                self.provision_mark_capacity_edit(id);
                self.provision_mark_source_password_unverified(id);
                self.provision.message = None;
            }
        }
    }
}
