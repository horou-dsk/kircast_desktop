mod ffmpeg_audio;
mod ffmpeg_sdl;

use std::cell::UnsafeCell;

use airplay2_protocol::airplay::airplay_consumer::AirPlayConsumer;
use airplay2_protocol::airplay::lib::audio_stream_info::CompressionType;
use airplay2_protocol::airplay::server::AudioPacket;
#[cfg(windows)]
use windows_sys::Win32::System::Power::{
    SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED,
};

use self::{ffmpeg_audio::FfMpegAudio, ffmpeg_sdl::SdlFfmpeg};

pub struct VideoConsumer {
    audio_compression_type: UnsafeCell<CompressionType>,
    ffmpeg: SdlFfmpeg,
    ffmpeg_audio: FfMpegAudio,
}

unsafe impl Sync for VideoConsumer {}

impl Default for VideoConsumer {
    fn default() -> Self {
        Self {
            audio_compression_type: CompressionType::Alac.into(),
            ffmpeg: SdlFfmpeg::new(1920, 1080),
            ffmpeg_audio: FfMpegAudio::default(),
        }
    }
}

impl AirPlayConsumer for VideoConsumer {
    fn on_video(&self, bytes: &[u8]) {
        if let Err(err) = self.ffmpeg.push_buffer(bytes) {
            log::error!("ffmpeg push_buffer error! {:?}", err);
        }
    }

    fn on_video_format(
        &self,
        video_stream_info: airplay2_protocol::airplay::lib::video_stream_info::VideoStreamInfo,
    ) {
        if cfg!(windows) {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED);
            }
        }
        self.ffmpeg.start().expect("ffmpeg start error");
        log::info!(
            "OnVideo Format... {:?}",
            video_stream_info.get_stream_connection_id()
        );
    }

    fn on_video_src_disconnect(&self) {
        log::info!("OnVideo Disconnect...");
        if cfg!(windows) {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
        }
        self.ffmpeg.stop();
    }

    fn on_audio_format(
        &self,
        audio_stream_info: airplay2_protocol::airplay::lib::audio_stream_info::AudioStreamInfo,
    ) {
        log::info!("audio_stream_info... = {:#?}", audio_stream_info);
        self.ffmpeg_audio
            .set_samples_per_frame(audio_stream_info.samples_per_frame);
        let result = match audio_stream_info.compression_type {
            CompressionType::Alac => self.ffmpeg_audio.start_alac(),
            _ => self.ffmpeg_audio.start_aac(),
        };
        unsafe { *self.audio_compression_type.get() = audio_stream_info.compression_type };
        if let Err(err) = result {
            log::error!("start audio error {err:?}");
        }
    }

    fn on_audio(&self, packet: &AudioPacket) {
        if let Err(err) = self.ffmpeg_audio.push_buffer(packet) {
            log::error!("ffmpeg_audio push_buffer error {err:?}");
        }
    }

    fn on_audio_src_disconnect(&self) {
        log::info!("OnAudio Disconnect...");
        self.ffmpeg_audio.stop();
    }

    fn on_volume(&self, volume: f32) {
        let volume = volume / 30.0 + 1.0;
        if let Err(err) = self.ffmpeg_audio.set_volume(volume) {
            log::error!("set volume error {err:?}");
        }
    }
}
