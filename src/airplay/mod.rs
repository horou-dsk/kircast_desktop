mod ffmpeg_audio;
mod ffmpeg_sdl;

use std::{cell::UnsafeCell, str::FromStr};

use airplay2_protocol::airplay::airplay_consumer::AirPlayConsumer;
use airplay2_protocol::airplay::lib::audio_stream_info::CompressionType;
use gst::Caps;
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::{AppSrc, AppStreamType};
use windows_sys::Win32::System::Power::{
    SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED,
};

use self::ffmpeg_sdl::SdlFfmpeg;

pub struct VideoConsumer {
    alac: (gst::Pipeline, AppSrc, gst::Element),
    aac_eld: (gst::Pipeline, AppSrc, gst::Element),
    audio_compression_type: UnsafeCell<CompressionType>,
    ffmpeg: SdlFfmpeg,
}

unsafe impl Sync for VideoConsumer {}

impl Default for VideoConsumer {
    fn default() -> Self {
        gst::init().unwrap();

        let caps = Caps::from_str("audio/x-alac,mpegversion=(int)4,channels=(int)2,rate=(int)48000,stream-format=raw,codec_data=(buffer)00000024616c616300000000000001600010280a0e0200ff00000000000000000000ac44").unwrap();
        let alac_pipeline = gst::Pipeline::default();

        let alac_appsrc = AppSrc::builder()
            .is_live(true)
            .stream_type(AppStreamType::Stream)
            .caps(&caps)
            .format(gst::Format::Time)
            .build();

        let alac_volume = gst::ElementFactory::make("volume").build().unwrap();
        let avdec_alac = gst::ElementFactory::make("avdec_alac").build().unwrap();
        let audioconvert = gst::ElementFactory::make("audioconvert").build().unwrap();
        let audioresample = gst::ElementFactory::make("audioresample").build().unwrap();
        let autoaudiosink = gst::ElementFactory::make("autoaudiosink")
            .property("sync", false)
            .build()
            .unwrap();

        alac_pipeline
            .add_many(&[
                alac_appsrc.upcast_ref(),
                &alac_volume,
                &avdec_alac,
                &audioconvert,
                &audioresample,
                &autoaudiosink,
            ])
            .unwrap();
        gst::Element::link_many(&[
            alac_appsrc.upcast_ref(),
            &avdec_alac,
            &audioconvert,
            &alac_volume,
            &audioresample,
            &autoaudiosink,
        ])
        .unwrap();

        let caps = Caps::from_str("audio/mpeg,mpegversion=(int)4,channnels=(int)2,rate=(int)44100,stream-format=raw,codec_data=(buffer)f8e85000").unwrap();
        let aac_eld_pipeline = gst::Pipeline::default();

        let aac_eld_appsrc = AppSrc::builder()
            .is_live(true)
            .stream_type(AppStreamType::Stream)
            .caps(&caps)
            .format(gst::Format::Time)
            .build();
        let aac_eld_volume = gst::ElementFactory::make("volume").build().unwrap();
        let avdec_aac = gst::ElementFactory::make("avdec_aac").build().unwrap();
        let audioconvert = gst::ElementFactory::make("audioconvert").build().unwrap();
        let audioresample = gst::ElementFactory::make("audioresample").build().unwrap();
        let autoaudiosink = gst::ElementFactory::make("autoaudiosink")
            .property("sync", false)
            .build()
            .unwrap();
        aac_eld_pipeline
            .add_many(&[
                aac_eld_appsrc.upcast_ref(),
                &avdec_aac,
                &audioconvert,
                &aac_eld_volume,
                &audioresample,
                &autoaudiosink,
            ])
            .unwrap();
        gst::Element::link_many(&[
            aac_eld_appsrc.upcast_ref(),
            &avdec_aac,
            &audioconvert,
            &aac_eld_volume,
            &audioresample,
            &autoaudiosink,
        ])
        .unwrap();

        Self {
            alac: (alac_pipeline, alac_appsrc, alac_volume),
            aac_eld: (aac_eld_pipeline, aac_eld_appsrc, aac_eld_volume),
            audio_compression_type: CompressionType::Alac.into(),
            ffmpeg: SdlFfmpeg::new(1920, 1080),
        }
    }
}

impl AirPlayConsumer for VideoConsumer {
    fn on_video(&self, bytes: &[u8]) {
        // unsafe {
        //     let writer = H264_FILE.as_mut().unwrap();
        //     writer.write_all(bytes).unwrap();
        // }
        if let Err(err) = self.ffmpeg.push_buffer(bytes) {
            log::error!("ffmpeg push_buffer error! {:?}", err);
        }
        // let buffer = gst::Buffer::from_slice(bytes.to_vec());
        // self.h264.1.push_buffer(buffer).ok();
    }

    fn on_video_format(
        &self,
        video_stream_info: airplay2_protocol::airplay::lib::video_stream_info::VideoStreamInfo,
    ) {
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED);
            // let file = File::create("./tmp/h264").unwrap();
            // H264_FILE = Some(BufWriter::new(file));
        }
        self.ffmpeg.start().expect("ffmpeg start error");
        // self.h264
        //     .0
        //     .set_state(gst::State::Playing)
        //     .expect("Unable to set the pipeline to the `Playing` state");
        log::info!(
            "OnVideo Format... {:?}",
            video_stream_info.get_stream_connection_id()
        );
    }

    fn on_video_src_disconnect(&self) {
        log::info!("OnVideo Disconnect...");
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS);
            // let mut writer = H264_FILE.take().unwrap();
            // writer.flush().unwrap();
        }
        self.ffmpeg.stop();
        // self.h264
        //     .0
        //     .set_state(gst::State::Null)
        //     .expect("Unable to set the pipeline to the `Null` state");
    }

    fn on_audio_format(
        &self,
        audio_stream_info: airplay2_protocol::airplay::lib::audio_stream_info::AudioStreamInfo,
    ) {
        log::info!(
            "on_audio_format... type = {:?}",
            audio_stream_info.compression_type
        );
        unsafe { *self.audio_compression_type.get() = audio_stream_info.compression_type };
        self.alac
            .0
            .set_state(gst::State::Playing)
            .expect("Unable to set the pipeline to the `Playing` state");
        self.aac_eld
            .0
            .set_state(gst::State::Playing)
            .expect("Unable to set the pipeline to the `Playing` state");
    }

    fn on_audio(&self, bytes: &[u8]) {
        let buffer = gst::Buffer::from_slice(bytes.to_vec());
        match unsafe { &*self.audio_compression_type.get() } {
            CompressionType::Alac => {
                self.alac.1.push_buffer(buffer).ok();
            }
            _ => {
                self.aac_eld.1.push_buffer(buffer).ok();
            }
        }
    }

    fn on_audio_src_disconnect(&self) {
        log::info!("OnAudio Disconnect...");
        self.alac
            .0
            .set_state(gst::State::Null)
            .expect("Unable to set the pipeline to the `Null` state");
        self.aac_eld
            .0
            .set_state(gst::State::Null)
            .expect("Unable to set the pipeline to the `Null` state");
    }

    fn on_volume(&self, volume: f32) {
        let volume = volume / 30.0 + 1.0;
        match unsafe { &*self.audio_compression_type.get() } {
            CompressionType::Alac => {
                self.alac.2.set_property("volume", volume as f64);
            }
            _ => {
                self.aac_eld.2.set_property("volume", volume as f64);
            }
        }
    }
}
