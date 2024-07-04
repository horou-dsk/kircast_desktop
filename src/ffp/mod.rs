use ffmpeg_sys_next::{AVCodecID, AVCodecParameters};

extern "C" {
    pub fn ff_audio_codec_par(
        codec_id: AVCodecID,
        buf: *const u8,
        len: usize,
        sample_rate: i32,
        channels: i32,
    ) -> *mut AVCodecParameters;
}
