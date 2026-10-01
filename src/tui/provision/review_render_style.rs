use super::*;
use crate::tui::state::ProvisionConfirmationAction;

pub(super) fn action_style(action: ProvisionConfirmationAction) -> Style {
    match action {
        ProvisionConfirmationAction::Preserve
        | ProvisionConfirmationAction::Passthrough
        | ProvisionConfirmationAction::Rewrap => success(),
        ProvisionConfirmationAction::Migrate => accent(),
        ProvisionConfirmationAction::FormatRebuild | ProvisionConfirmationAction::New => warning(),
        ProvisionConfirmationAction::Delete => danger(),
    }
}
