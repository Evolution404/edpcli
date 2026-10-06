//! Owned secret text with redacted diagnostics and explicit allocation cleanup.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SecretText(String);

pub(crate) fn wipe(bytes: &mut [u8]) {
    for byte in bytes {
        // SAFETY: exclusive live allocation; volatile stores prevent dead-store elimination.
        unsafe {
            std::ptr::write_volatile(byte, 0);
        }
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}
/// Wipe allocated capacity, including removed bytes and spare storage, without reading it.
pub(crate) fn wipe_vec(bytes: &mut Vec<u8>) {
    for index in 0..bytes.capacity() {
        // SAFETY: every byte of this allocation is writable, including spare capacity.
        unsafe {
            std::ptr::write_volatile(bytes.as_mut_ptr().add(index), 0);
        }
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}
pub(crate) fn reserve_secret(bytes: &mut Vec<u8>, additional: usize) {
    let needed = bytes
        .len()
        .checked_add(additional)
        .expect("secret capacity overflow");
    if needed > bytes.capacity() {
        let mut replacement = Vec::with_capacity(needed.max(bytes.capacity().saturating_mul(2)));
        replacement.extend_from_slice(bytes);
        wipe_vec(bytes);
        *bytes = replacement;
    }
}
impl SecretText {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn insert_char(&mut self, index: usize, ch: char) -> bool {
        let offset = self
            .0
            .char_indices()
            .nth(index)
            .map(|(offset, _)| offset)
            .or_else(|| (index == self.0.chars().count()).then_some(self.0.len()));
        let Some(offset) = offset else {
            return false;
        };
        // SAFETY: reserve does not alter length or UTF-8 bytes and wipes the old allocation before replacement.
        unsafe {
            reserve_secret(self.0.as_mut_vec(), ch.len_utf8());
        }
        self.0.insert(offset, ch);
        true
    }
    pub fn remove_char(&mut self, index: usize) -> bool {
        let Some((offset, _)) = self.0.char_indices().nth(index) else {
            return false;
        };
        let old_len = self.0.len();
        self.0.remove(offset);
        // SAFETY: removed trailing bytes remain allocated; wipe them before reuse.
        unsafe {
            let bytes = self.0.as_mut_vec();
            for index in bytes.len()..old_len {
                std::ptr::write_volatile(bytes.as_mut_ptr().add(index), 0);
            }
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
        true
    }
    pub fn push(&mut self, ch: char) {
        self.insert_char(self.0.chars().count(), ch);
    }
    pub fn pop(&mut self) {
        if let Some(index) = self.0.chars().count().checked_sub(1) {
            self.remove_char(index);
        }
    }
    /// Transfer the allocation directly to the next secret owner.
    pub fn into_bytes(mut self) -> Vec<u8> {
        std::mem::take(&mut self.0).into_bytes()
    }
    pub fn clear(&mut self) {
        // SAFETY: zeros are valid UTF-8 and the vector keeps its allocation/length invariant.
        unsafe {
            wipe_vec(self.0.as_mut_vec());
        }
        self.0.clear();
    }
}
impl Drop for SecretText {
    fn drop(&mut self) {
        self.clear();
    }
}
impl From<String> for SecretText {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl From<&str> for SecretText {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}
impl std::fmt::Debug for SecretText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}
impl PartialEq<&str> for SecretText {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}
impl PartialEq<String> for SecretText {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_and_clears_owned_storage() {
        let mut value = SecretText::from("example-secret");
        assert_eq!(format!("{value:?}"), "[REDACTED]");
        value.clear();
        assert!(value.is_empty());
        let mut bytes = [1, 2, 3];
        wipe(&mut bytes);
        assert_eq!(bytes, [0; 3]);
    }
}

#[cfg(test)]
mod mutation_tests {
    use super::*;
    #[test]
    fn edits_wipe_removed_utf8_and_all_capacity() {
        let mut value = SecretText::from("a密码z");
        let old_len = value.0.len();
        assert!(value.remove_char(1));
        assert_eq!(value.as_str(), "a码z");
        for index in value.0.len()..old_len {
            assert_eq!(unsafe { *value.0.as_ptr().add(index) }, 0);
        }
        assert!(value.insert_char(1, '秘'));
        value.pop();
        assert_eq!(value.as_str(), "a秘码");
        value.clear();
        for index in 0..value.0.capacity() {
            assert_eq!(unsafe { *value.0.as_ptr().add(index) }, 0);
        }
    }
}

/// CLI-owned argument copies are wiped on normal returns, including elevation errors.
pub(crate) struct SecretArguments(pub(crate) Vec<String>);
impl std::ops::Deref for SecretArguments {
    type Target = Vec<String>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for SecretArguments {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for SecretArguments {
    fn drop(&mut self) {
        for arg in &mut self.0 {
            // SAFETY: zeros preserve UTF-8 validity; the allocation remains exclusively owned.
            unsafe {
                wipe_vec(arg.as_mut_vec());
            }
        }
    }
}
