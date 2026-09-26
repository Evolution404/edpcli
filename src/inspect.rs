//! 只读扇区检查器：把物理盘与备份镜像统一成同一套“解密 → 结构解析 → 字段高亮”视图。
//! 本模块不做任何写盘动作，也不负责提权；CLI 只在物理盘来源时请求裸盘读取权限。

use std::collections::BTreeSet;

use encoding_rs::GBK;

use crate::common::SECTOR;
use crate::crypto::{crc32_bare, lba6_checksum};
use crate::protocol::{
    edpf::{EdpfEntry64, EdpfEntry96, PassInfo},
    lba0, lba1, lba10, lba11, lba12, lba2, lba3, lba4, lba5, lba6, lba7, lba8, lba9,
    profile::{
        DeptLayout, HostHardinfoSource, Lba10Eesi, Lba11Capacity, Lba12Mode, Lba7EntryCount,
        Lba7PassinfoVersion, Lba8UsbOnlyInfo,
    },
};

mod catalog;
mod lba_adapter;
mod lba_early;
mod lba_late;
mod lba_middle;
mod metadata;
mod model;
mod render;

pub use lba_adapter::{analyze_sector, analyze_sector_with_context};
pub use metadata::InspectMeta;
pub use model::{FieldChild, FieldStyle, SectorField, SectorView};
pub use render::{overview_line, render_fields, render_hex};

use metadata::*;
use model::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_info_uses_official_password_complexity_skip_label() {
        let pass = PassInfo {
            version: 0x0206,
            force_change_share: 0,
            max_share_password_errors: 0,
            current_share_password_errors: 0,
            force_change_encrypt: 0,
            max_encrypt_password_errors: 0,
            current_encrypt_password_errors: 0,
            no_password_set: 0,
            no_password_no_check_ip: 0,
            no_usb_check_password_safe: 1,
            reset_file_key: 0,
            share_backup_prompt_period: 0,
            encrypt_backup_prompt_period: 0,
        };
        let fields = pass_info_fields(0xc0, &pass, "PassInfo");
        let field = fields
            .iter()
            .find(|field| field.label == "取消密码复杂性验证")
            .expect("official complexity-skip field must be exposed");
        assert_eq!(field.start, 0xca);
        assert_eq!(field.end, 0xcb);
        assert_eq!(field.value, "1");
    }

    #[test]
    fn truncated_legacy_gbk_keeps_readable_prefix() {
        let mut raw = b"Dept=*^$@".to_vec();
        raw.extend_from_slice(&[
            0xBD, 0xAD, 0xCB, 0xD5, 0xCA, 0xA1, 0xB5, 0xE7, 0xC1, 0xA6, 0xD3, 0xD0, 0xCF, 0xDE,
            0xB9, 0xAB, 0xCB, 0xBE, 0x2F, 0xBD,
        ]);
        let decoded = text_value(&raw);
        assert!(
            decoded.starts_with("Dept=*^$@江苏省电力有限公司/"),
            "{decoded}"
        );
        assert!(!decoded.contains("[hex:"), "{decoded}");
    }
}
