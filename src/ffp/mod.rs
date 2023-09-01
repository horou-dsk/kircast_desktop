use ffmpeg_sys_next::AVCodecParameters;

extern "C" {
    pub fn ff_alac_par(buf: *const u8, len: usize) -> *mut AVCodecParameters;
    pub fn ff_aac_par(buf: *const u8, len: usize) -> *mut AVCodecParameters;
}
