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

impl PasswordIntent {
    pub(crate) const fn target_password_requested(self) -> bool {
        matches!(self, Self::Rewrap | Self::Rebuild)
    }

    pub(crate) fn blocking_message(self, label: &str) -> Option<String> {
        match self {
            Self::Waiting => Some(format!(
                "{label}原密码正在只读验证，请稍候再生成计划"
            )),
            Self::BlockedNeedsFormat => Some(format!(
                "{label}原密码未验证，不能无损改密；如需使用新密码，请主动勾选{label}格式化，或按 Space 切回透传"
            )),
            Self::BlockedNeedsExplicitPassword => Some(format!(
                "{label}已选择格式化，但原密码未验证；请按 i 设置新密码，或取消格式化以继续透传"
            )),
            Self::Passthrough | Self::Rewrap | Self::Rebuild => None,
        }
    }
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
