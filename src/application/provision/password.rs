#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TargetPasswordMode {
    #[default]
    Passthrough,
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcePasswordState {
    NotApplicable,
    Verifying,
    VerifiedDefault,
    VerifiedUser,
    Failed,
    Unknown,
}

impl SourcePasswordState {
    pub const fn is_verified(self) -> bool {
        matches!(self, Self::VerifiedDefault | Self::VerifiedUser)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordIntent {
    InitializeNew,
    Waiting,
    Passthrough,
    Rewrap,
    Rebuild,
    BlockedNeedsFormat,
    BlockedNeedsExplicitPassword,
}

pub fn decide_password_intent(
    source_state: SourcePasswordState,
    target_mode: TargetPasswordMode,
    source_password: &str,
    target_password: &str,
    format_selected: bool,
) -> PasswordIntent {
    if source_state == SourcePasswordState::NotApplicable {
        return if target_mode == TargetPasswordMode::Explicit && !target_password.is_empty() {
            PasswordIntent::InitializeNew
        } else {
            PasswordIntent::BlockedNeedsExplicitPassword
        };
    }
    if format_selected {
        if target_mode == TargetPasswordMode::Explicit {
            return if target_password.is_empty() {
                PasswordIntent::BlockedNeedsExplicitPassword
            } else {
                PasswordIntent::Rebuild
            };
        }
        return if source_state.is_verified() {
            PasswordIntent::Rebuild
        } else {
            PasswordIntent::BlockedNeedsExplicitPassword
        };
    }
    if source_state == SourcePasswordState::Verifying {
        return PasswordIntent::Waiting;
    }
    match target_mode {
        TargetPasswordMode::Passthrough => PasswordIntent::Passthrough,
        TargetPasswordMode::Explicit if !source_state.is_verified() => {
            PasswordIntent::BlockedNeedsFormat
        }
        TargetPasswordMode::Explicit if target_password == source_password => {
            PasswordIntent::Passthrough
        }
        TargetPasswordMode::Explicit => PasswordIntent::Rewrap,
    }
}
