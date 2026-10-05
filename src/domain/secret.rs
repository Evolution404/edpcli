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
    pub fn clear(&mut self) {
        // SAFETY: zeros are valid UTF-8 and the vector keeps its allocation/length invariant.
        unsafe {
            wipe(self.0.as_mut_vec());
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
