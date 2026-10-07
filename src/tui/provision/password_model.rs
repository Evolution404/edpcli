pub(crate) use crate::application::provision::password::{
    decide_password_intent, PasswordIntent, SourcePasswordState, TargetPasswordMode,
};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TargetPasswordModeState {
    pub(super) share: TargetPasswordMode,
    pub(super) encrypt: TargetPasswordMode,
    pub(super) share_draft_user_edited: bool,
    pub(super) encrypt_draft_user_edited: bool,
}
