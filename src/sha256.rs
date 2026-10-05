//! SHA-256 digest helpers for backup integrity and deterministic test fingerprints.

use sha2::{Digest, Sha256};

/// Compute SHA-256 and return lowercase hexadecimal text.
pub fn sha256_hex(data: &[u8]) -> String {
    crate::common::hex_lower(&Sha256::digest(data))
}

/// Hash a stream with bounded memory and a finite byte budget, including growth.
pub fn sha256_reader_hex(reader: &mut impl std::io::Read, maximum: u64) -> std::io::Result<String> {
    use std::io::{Error, ErrorKind};
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut count = 0u64;
    loop {
        let read = match reader.read(&mut buffer) {
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            break;
        }
        count = count
            .checked_add(read as u64)
            .filter(|value| *value <= maximum)
            .ok_or_else(|| Error::other("file exceeds hash read budget"))?;
        hasher.update(&buffer[..read]);
    }
    Ok(crate::common::hex_lower(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_hash_checks_read_budget_even_for_non_file_readers() {
        let bytes = vec![0x5a; 128 * 1024 + 1];
        let mut reader = std::io::Cursor::new(&bytes);
        assert_eq!(
            sha256_reader_hex(&mut reader, bytes.len() as u64).unwrap(),
            sha256_hex(&bytes)
        );
        reader.set_position(0);
        assert!(sha256_reader_hex(&mut reader, bytes.len() as u64 - 1)
            .unwrap_err()
            .to_string()
            .contains("exceeds hash read budget"));
    }

    #[test]
    fn standard_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
