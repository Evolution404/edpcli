use super::*;
use crate::tui::state::{
    ProvisionConfirmationAction, ProvisionConfirmationDataEffect,
    ProvisionConfirmationFilesystemEffect, ProvisionConfirmationPasswordEffect,
};

pub(super) fn action_style(action: ProvisionConfirmationAction) -> Style {
    match action {
        ProvisionConfirmationAction::Fixed | ProvisionConfirmationAction::Free => muted(),
        ProvisionConfirmationAction::Preserve
        | ProvisionConfirmationAction::Passthrough
        | ProvisionConfirmationAction::Rewrap => success(),
        ProvisionConfirmationAction::FormatRebuild | ProvisionConfirmationAction::New => warning(),
        ProvisionConfirmationAction::Delete => danger(),
    }
}

pub(super) fn data_style(effect: ProvisionConfirmationDataEffect) -> Style {
    match effect {
        ProvisionConfirmationDataEffect::Preserve => success(),
        ProvisionConfirmationDataEffect::Clear => warning(),
        ProvisionConfirmationDataEffect::None => muted(),
    }
}

pub(super) fn password_style(effect: ProvisionConfirmationPasswordEffect) -> Style {
    match effect {
        ProvisionConfirmationPasswordEffect::None => muted(),
        ProvisionConfirmationPasswordEffect::Preserve => success(),
        ProvisionConfirmationPasswordEffect::Rewrap => accent(),
        ProvisionConfirmationPasswordEffect::InitializeNew => accent(),
        ProvisionConfirmationPasswordEffect::Rebuild => warning(),
    }
}

pub(super) fn filesystem_style(effect: ProvisionConfirmationFilesystemEffect) -> Style {
    match effect {
        ProvisionConfirmationFilesystemEffect::Keep => success(),
        ProvisionConfirmationFilesystemEffect::Format(_)
        | ProvisionConfirmationFilesystemEffect::Create(_) => warning(),
        ProvisionConfirmationFilesystemEffect::None => muted(),
    }
}
