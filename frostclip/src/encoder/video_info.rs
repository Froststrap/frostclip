#[derive(Clone)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,

    pub time_base_num: i32,
    pub time_base_den: i32,

    pub codec: ffmpeg_next::codec::Id,
    pub format: ffmpeg_next::format::Pixel,

    pub extradata: Vec<u8>,
}
