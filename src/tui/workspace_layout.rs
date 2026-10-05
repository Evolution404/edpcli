//! Geometry shared by desktop rendering and page navigation.
pub(crate) fn device_list_height(height: u16, count: usize) -> u16 {
    let desired = count.saturating_add(6).clamp(9, usize::from(u16::MAX)) as u16;
    desired
        .min(height.saturating_mul(44) / 100)
        .max(7)
        .min(height)
}
