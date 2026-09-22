use edpcli::protocol::{layout, profile::*, types::*};

#[test]
pub fn core_profiles_and_catalog_are_typed_and_independent() {
    let mut profile = ProtocolProfile::default();
    assert_eq!(profile.lba4_encoding, Lba4Encoding::Unknown);
    profile.dept_layout = DeptLayout::Join59;
    assert_eq!(profile.lba4_encoding, Lba4Encoding::Unknown);
    assert_eq!(profile.dept_layout.as_str(), "join59");
    assert_eq!(
        Lba4Encoding::from_state("ordinary-rolling"),
        Some(Lba4Encoding::OrdinaryRolling)
    );
    assert_eq!(Lba4Encoding::from_state("garbage"), None);
    assert_eq!(layout::fields().len(), 142);
    let field = layout::field("lba3.manufacturer_metadata", "zero").unwrap();
    assert_eq!(field.semantic_type, SemanticType::OpaquePreserve);
    assert_eq!(field.ownership, Ownership::Owner);
    assert_eq!(field.length(), 512);
    assert!(WireSector::try_from(&[0u8; 511][..]).is_err());
    assert!(WireSector::try_from(&[0u8; 513][..]).is_err());
}
