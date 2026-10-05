//! Errors retain machine information until the frontend chooses a presentation.
use crate::common::EdpCliError;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationError {
    pub code: Option<i32>,
    pub phase: &'static str,
    pub message: String,
}
impl OperationError {
    pub fn in_phase(mut self, phase: &'static str) -> Self {
        self.phase = phase;
        self
    }
}
impl From<EdpCliError> for OperationError {
    fn from(error: EdpCliError) -> Self {
        Self {
            code: Some(error.code),
            phase: "操作",
            message: error.msg,
        }
    }
}
impl From<String> for OperationError {
    fn from(message: String) -> Self {
        Self {
            code: None,
            phase: "后台任务",
            message,
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
}
