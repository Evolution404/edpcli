#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TargetPasswordMode {
    #[default]
    Passthrough,
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourcePasswordState {
    Verifying,
    VerifiedDefault,
    VerifiedUser,
    Failed,
    Unknown,
}

impl SourcePasswordState {
    pub(crate) const fn is_verified(self) -> bool {
        matches!(self, Self::VerifiedDefault | Self::VerifiedUser)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PasswordIntent {
    Waiting,
    Passthrough,
    Rewrap,
    Rebuild,
    BlockedNeedsFormat,
    BlockedNeedsExplicitPassword,
}

pub(crate) fn decide_password_intent(
    source_state: SourcePasswordState,
    target_mode: TargetPasswordMode,
    source_password: &str,
    target_password: &str,
    format_selected: bool,
) -> PasswordIntent {
    if source_state == SourcePasswordState::Verifying {
        return PasswordIntent::Waiting;
    }
    if format_selected {
        if !source_state.is_verified() && target_mode == TargetPasswordMode::Passthrough {
            return PasswordIntent::BlockedNeedsExplicitPassword;
        }
        return PasswordIntent::Rebuild;
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

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TargetPasswordModeState {
    pub(super) share: TargetPasswordMode,
    pub(super) encrypt: TargetPasswordMode,
    pub(super) share_draft_user_edited: bool,
    pub(super) encrypt_draft_user_edited: bool,
}
