//! Per-domain password and FileKey planning primitives.
//!
//! Passwords are deliberately scoped to semantic key domains rather than a
//! whole disk. This module owns secret lifetimes only; protocol wrapping stays
//! in `keys.rs`.

use std::fmt;

use super::PartitionRole;

pub const DEFAULT_KEY_DOMAIN_PASSWORD: &[u8] = b"0000aaaa";

#[derive(Clone, Eq, PartialEq)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub fn new(value: impl AsRef<[u8]>) -> Self {
        Self(value.as_ref().to_vec())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Default for SecretBytes {
    fn default() -> Self {
        Self::new([])
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretBytes([REDACTED])")
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum KeyDomainRole {
    Share,
    Encrypt,
}

impl KeyDomainRole {
    pub const fn from_partition_role(role: PartitionRole) -> Option<Self> {
        match role {
            PartitionRole::Share | PartitionRole::BootShareCombined => Some(Self::Share),
            PartitionRole::Encrypt => Some(Self::Encrypt),
            PartitionRole::Boot | PartitionRole::CompatibilityReserve => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SourcePasswordKnowledge {
    DefaultVerified,
    UserVerified,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TargetPasswordPolicy {
    #[default]
    PreserveOpaque,
    ReuseVerified,
    ReplaceVerified,
    InitializeNew,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeyDomainSecretPair {
    pub source_password: Option<SecretBytes>,
    pub target_password: Option<SecretBytes>,
}

impl KeyDomainSecretPair {
    pub fn new(
        source_password: Option<impl AsRef<[u8]>>,
        target_password: Option<impl AsRef<[u8]>>,
    ) -> Self {
        Self {
            source_password: source_password.map(SecretBytes::new),
            target_password: target_password.map(SecretBytes::new),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeyDomainSecrets {
    pub share: KeyDomainSecretPair,
    pub encrypt: KeyDomainSecretPair,
}

impl KeyDomainSecrets {
    pub fn new(share: KeyDomainSecretPair, encrypt: KeyDomainSecretPair) -> Self {
        Self { share, encrypt }
    }

    pub fn default_targets() -> Self {
        Self {
            share: KeyDomainSecretPair::new(
                None::<&[u8]>,
                Some(DEFAULT_KEY_DOMAIN_PASSWORD),
            ),
            encrypt: KeyDomainSecretPair::new(
                None::<&[u8]>,
                Some(DEFAULT_KEY_DOMAIN_PASSWORD),
            ),
        }
    }

    pub fn pair(&self, role: KeyDomainRole) -> &KeyDomainSecretPair {
        match role {
            KeyDomainRole::Share => &self.share,
            KeyDomainRole::Encrypt => &self.encrypt,
        }
    }

    pub fn pair_for_partition(&self, role: PartitionRole) -> Option<&KeyDomainSecretPair> {
        KeyDomainRole::from_partition_role(role).map(|domain| self.pair(domain))
    }

    pub fn source_password(&self, role: PartitionRole) -> Option<&[u8]> {
        self.pair_for_partition(role)?
            .source_password
            .as_ref()
            .map(SecretBytes::as_bytes)
    }

    pub fn target_password(&self, role: PartitionRole) -> Option<&[u8]> {
        self.pair_for_partition(role)?
            .target_password
            .as_ref()
            .map(SecretBytes::as_bytes)
    }
}
