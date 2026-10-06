//! Errors retain machine information until the frontend chooses a presentation.
use crate::common::{EdpCliError, EXIT_INTERMEDIATE, EXIT_ROLLED_BACK};

/// State of the scope touched by a failed operation, not the whole multi-stage operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaState {
    Unchanged,
    RolledBack,
    Intermediate,
    Unknown,
}

impl MediaState {
    pub const fn requires_reinspection(self) -> bool {
        matches!(self, Self::Intermediate | Self::Unknown)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationError {
    pub code: Option<i32>,
    pub phase: &'static str,
    pub message: String,
    pub media_state: Option<MediaState>,
}
impl OperationError {
    pub fn exit_code(&self) -> i32 {
        if self
            .media_state
            .is_some_and(MediaState::requires_reinspection)
        {
            EXIT_INTERMEDIATE
        } else {
            self.code.unwrap_or(crate::common::EXIT_IO)
        }
    }

    pub fn in_phase(mut self, phase: &'static str) -> Self {
        self.phase = phase;
        self
    }

    pub fn with_media_state(mut self, state: MediaState) -> Self {
        self.media_state = Some(state);
        self
    }
}
impl From<EdpCliError> for OperationError {
    fn from(error: EdpCliError) -> Self {
        let media_state = match error.code {
            EXIT_INTERMEDIATE => Some(MediaState::Intermediate),
            EXIT_ROLLED_BACK => Some(MediaState::RolledBack),
            _ => None,
        };
        Self {
            code: Some(error.code),
            phase: "操作",
            message: error.msg,
            media_state,
        }
    }
}
impl From<String> for OperationError {
    fn from(message: String) -> Self {
        Self {
            code: None,
            phase: "后台任务",
            message,
            media_state: None,
        }
    }
}
impl From<&str> for OperationError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
impl std::fmt::Display for OperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(code) = self.code {
            write!(f, "{}：{}（错误码 {}）", self.phase, self.message, code)
        } else {
            write!(f, "{}：{}", self.phase, self.message)
        }
    }
}
impl std::error::Error for OperationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_exit_code_and_operation_phase_until_frontend_boundary() {
        let error = OperationError::from(EdpCliError::new(7, "原始写入失败")).in_phase("恢复");
        assert_eq!(error.code, Some(7));
        assert_eq!(error.phase, "恢复");
        assert_eq!(error.message, "原始写入失败");
        assert_eq!(error.to_string(), "恢复：原始写入失败（错误码 7）");
    }

    #[test]
    fn background_text_error_remains_typed_without_fabricating_exit_code() {
        let error = OperationError::from("后台任务异常").in_phase("备份后台任务");
        assert_eq!(error.code, None);
        assert_eq!(error.phase, "备份后台任务");
        assert_eq!(error.to_string(), "备份后台任务：后台任务异常");
    }

    #[test]
    fn media_uncertainty_takes_precedence_over_original_io_exit_code() {
        let error = OperationError::from(EdpCliError::new(
            crate::common::EXIT_IO,
            "write completed, verify failed",
        ))
        .with_media_state(MediaState::Unknown);
        assert_eq!(error.code, Some(crate::common::EXIT_IO));
        assert_eq!(error.exit_code(), EXIT_INTERMEDIATE);
        let rolled_back =
            OperationError::from(EdpCliError::new(EXIT_ROLLED_BACK, "rollback verified"));
        assert_eq!(rolled_back.media_state, Some(MediaState::RolledBack));
        assert_eq!(rolled_back.exit_code(), EXIT_ROLLED_BACK);
    }
}
