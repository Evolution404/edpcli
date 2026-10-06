//! Per-domain password and FileKey planning primitives.
//!
//! Passwords are deliberately scoped to semantic key domains rather than a
//! whole disk. This module owns secret lifetimes only; protocol wrapping stays
//! in `keys.rs`.

use std::fmt;

use super::PartitionRole;

pub const DEFAULT_KEY_DOMAIN_PASSWORD_TEXT: &str = "0000aaaa";
pub const DEFAULT_KEY_DOMAIN_PASSWORD: &[u8] = DEFAULT_KEY_DOMAIN_PASSWORD_TEXT.as_bytes();

#[derive(Clone, Eq, PartialEq)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub fn new(value: impl AsRef<[u8]>) -> Self {
        Self(value.as_ref().to_vec())
    }

    pub fn from_owned(value: Vec<u8>) -> Self {
        Self(value)
    }
    pub fn into_vec(mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
    pub fn clear(&mut self) {
        crate::domain::secret::wipe_vec(&mut self.0);
        self.0.clear();
    }
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        crate::domain::secret::reserve_secret(&mut self.0, bytes.len());
        self.0.extend_from_slice(bytes);
    }
    pub fn push_char(&mut self, ch: char) {
        let mut encoded = [0; 4];
        self.push_bytes(ch.encode_utf8(&mut encoded).as_bytes());
        crate::domain::secret::wipe(&mut encoded);
    }
    pub fn pop_char(&mut self) {
        if self.0.is_empty() {
            return;
        }
        let mut cut = self.0.len() - 1;
        while cut > 0 && self.0[cut] & 0xc0 == 0x80 {
            cut -= 1;
        }
        self.truncate(cut);
    }
    pub fn truncate(&mut self, length: usize) {
        if length < self.0.len() {
            crate::domain::secret::wipe(&mut self.0[length..]);
            self.0.truncate(length);
        }
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
        crate::domain::secret::wipe_vec(&mut self.0);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PassthroughBasis {
    Verified,
    OpaqueCompatible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PasswordDisposition {
    Passthrough(PassthroughBasis),
    Rewrap,
    Rebuild,
    Blocked,
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
            share: KeyDomainSecretPair::new(None::<&[u8]>, Some(DEFAULT_KEY_DOMAIN_PASSWORD)),
            encrypt: KeyDomainSecretPair::new(None::<&[u8]>, Some(DEFAULT_KEY_DOMAIN_PASSWORD)),
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

#[cfg(test)]
mod secret_mutation_tests {
    use super::*;
    #[test]
    fn removed_unicode_and_cancelled_bytes_are_wiped() {
        let mut secret = SecretBytes::new("secret密");
        let old_len = secret.0.len();
        secret.pop_char();
        assert_eq!(secret.as_bytes(), b"secret");
        for index in secret.0.len()..old_len {
            assert_eq!(unsafe { *secret.0.as_ptr().add(index) }, 0);
        }
        secret.push_char('码');
        secret.clear();
        for index in 0..secret.0.capacity() {
            assert_eq!(unsafe { *secret.0.as_ptr().add(index) }, 0);
        }
    }
}
