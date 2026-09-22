use super::types::OpaquePreserve;
#[derive(Clone, Debug)]
pub struct Lba5View {
    pub payload: OpaquePreserve<512>,
}
impl Lba5View {
    pub fn reconstruct(&self) -> [u8; 512] {
        self.payload.0
    }
}
pub fn parse_lba5(raw: &[u8; 512]) -> Lba5View {
    Lba5View {
        payload: OpaquePreserve(*raw),
    }
}
