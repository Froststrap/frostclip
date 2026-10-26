#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketKind {
    Video,
    Audio,
}

#[derive(Clone)]
pub struct EncodedPacket {
    pub kind: PacketKind,
    pub data: Vec<u8>,
    pub pts: i64,
    pub dts: i64,
    pub duration: i64,
    pub is_keyframe: bool,
}
