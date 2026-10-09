//! Canonical distinction between EDP protocol key metadata and physical data encryption.
//!
//! NeedEncrypt is an on-disk protocol/key-domain flag. It is NOT a direct statement
//! that the partition's physical sectors are encrypted. The canonical counterexample is
//! mode1 slot0 (BootShareCombined): type2 + NeedEncrypt=1 owns the Share key domain,
//! while its physical filesystem must remain plaintext and directly mountable.

use crate::protocol::{edpf::EdpPartitionType, lba7::Lba7PartitionMode};

use super::{KeyDomainRole, PartitionRole};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalPartitionEncryption {
    Plaintext,
    Sm4Sector,
}

impl PhysicalPartitionEncryption {
    pub const fn is_encrypted(self) -> bool {
        matches!(self, Self::Sm4Sector)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Plaintext => "物理明文",
            // Legacy variant describes the old write algorithm; source data
            // cipher is independently selected by the entry's EncryptMode.
            Self::Sm4Sector => "物理密文扇区（算法依 EncryptMode）",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OfficialPartitionSemantics {
    pub mode: Lba7PartitionMode,
    pub index: usize,
    pub partition_type: EdpPartitionType,
    pub role: PartitionRole,
    pub key_domain: Option<KeyDomainRole>,
    pub protocol_need_encrypt: bool,
    pub physical_encryption: PhysicalPartitionEncryption,
}

impl OfficialPartitionSemantics {
    pub const fn physically_encrypted(self) -> bool {
        self.physical_encryption.is_encrypted()
    }

    pub fn validate_protocol_need_encrypt(self, actual: u32) -> Result<(), String> {
        let expected = u32::from(self.protocol_need_encrypt);
        if actual == expected {
            Ok(())
        } else {
            Err(format!(
                "mode{} slot{} type{} NeedEncrypt={}，规范值应为 {}",
                self.mode as u8,
                self.index,
                self.partition_type.raw(),
                actual,
                expected,
            ))
        }
    }
}

pub const fn official_partition_role_semantics(
    mode: Lba7PartitionMode,
    index: usize,
    partition_type: EdpPartitionType,
) -> PartitionRole {
    match (mode, index, partition_type) {
        (Lba7PartitionMode::WholeDiskEncrypted, 0, EdpPartitionType::Boot) => {
            PartitionRole::CompatibilityReserve
        }
        (Lba7PartitionMode::BootShareCombined, 0, EdpPartitionType::Share) => {
            PartitionRole::BootShareCombined
        }
        (_, _, EdpPartitionType::Boot) => PartitionRole::Boot,
        (_, _, EdpPartitionType::Share) => PartitionRole::Share,
        (_, _, EdpPartitionType::Encrypt) => PartitionRole::Encrypt,
    }
}

pub const fn physical_partition_encryption_semantics(
    mode: Lba7PartitionMode,
    index: usize,
    partition_type: EdpPartitionType,
) -> PhysicalPartitionEncryption {
    match partition_type {
        EdpPartitionType::Boot => PhysicalPartitionEncryption::Plaintext,
        EdpPartitionType::Share
            if matches!(mode, Lba7PartitionMode::BootShareCombined) && index == 0 =>
        {
            PhysicalPartitionEncryption::Plaintext
        }
        EdpPartitionType::Share | EdpPartitionType::Encrypt => {
            PhysicalPartitionEncryption::Sm4Sector
        }
    }
}

pub const fn protocol_need_encrypt_semantics(partition_type: EdpPartitionType) -> bool {
    !matches!(partition_type, EdpPartitionType::Boot)
}

pub fn official_partition_semantics(
    mode: Lba7PartitionMode,
    index: usize,
    partition_type: EdpPartitionType,
) -> Result<OfficialPartitionSemantics, String> {
    let Some(expected) = mode.partition_types().get(index).copied() else {
        return Err(format!(
            "partition slot {index} is outside mode {}",
            mode as u8
        ));
    };
    if expected != partition_type {
        return Err(format!(
            "partition slot {index} type{} does not match mode {} type{}",
            partition_type.raw(),
            mode as u8,
            expected.raw()
        ));
    }
    let role = official_partition_role_semantics(mode, index, partition_type);
    Ok(OfficialPartitionSemantics {
        mode,
        index,
        partition_type,
        role,
        key_domain: KeyDomainRole::from_partition_role(role),
        protocol_need_encrypt: protocol_need_encrypt_semantics(partition_type),
        physical_encryption: physical_partition_encryption_semantics(mode, index, partition_type),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_matrix_keeps_protocol_need_encrypt_separate_from_physical_crypto() {
        for mode in [
            Lba7PartitionMode::DefaultThreePartition,
            Lba7PartitionMode::BootShareCombined,
            Lba7PartitionMode::WholeDiskEncrypted,
            Lba7PartitionMode::IntranetExtranetDualPartition,
        ] {
            for (index, partition_type) in mode.partition_types().iter().copied().enumerate() {
                let semantics = official_partition_semantics(mode, index, partition_type).unwrap();
                assert_eq!(
                    semantics.protocol_need_encrypt,
                    partition_type != EdpPartitionType::Boot
                );
            }
        }

        let combined = official_partition_semantics(
            Lba7PartitionMode::BootShareCombined,
            0,
            EdpPartitionType::Share,
        )
        .unwrap();
        assert_eq!(combined.role, PartitionRole::BootShareCombined);
        assert_eq!(combined.key_domain, Some(KeyDomainRole::Share));
        assert!(combined.protocol_need_encrypt);
        assert!(combined.validate_protocol_need_encrypt(1).is_ok());
        assert!(combined.validate_protocol_need_encrypt(0).is_err());
        assert_eq!(
            combined.physical_encryption,
            PhysicalPartitionEncryption::Plaintext
        );

        let mode0_share = official_partition_semantics(
            Lba7PartitionMode::DefaultThreePartition,
            1,
            EdpPartitionType::Share,
        )
        .unwrap();
        assert!(mode0_share.protocol_need_encrypt);
        assert_eq!(
            mode0_share.physical_encryption,
            PhysicalPartitionEncryption::Sm4Sector
        );
    }
}
