use std::str::FromStr;
use std::sync::RwLock;

use airplay2_protocol::airplay::airplay_consumer::AirPlayConsumer;
use airplay2_protocol::airplay::lib::audio_stream_info::CompressionType;
use gst::Caps;
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::{self as gst_app, AppSrc, AppStreamType};

pub struct VideoConsumer {
    alac: (gst::Pipeline, AppSrc),
    aac_eld: (gst::Pipeline, AppSrc),
    h264: (gst::Pipeline, AppSrc),
    audio_compression_type: RwLock<CompressionType>,
}

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
                &audioresample,
                &autoaudiosink,
            ])
            .unwrap();
        gst::Element::link_many(&[
            aac_eld_appsrc.upcast_ref(),
            &avdec_aac,
            &audioconvert,
            &audioresample,
            &autoaudiosink,
        ])
        .unwrap();

        let h264pipeline = gst::parse_launch(
            "appsrc name=h264-src ! h264parse ! avdec_h264 ! autovideosink sync=false",
        )
        .unwrap();

        let h264pipeline = h264pipeline.dynamic_cast::<gst::Pipeline>().unwrap();

        let mut h264_src = None;

        for elem in h264pipeline.children() {
            // println!("{}", elem.name());
            if elem.name() == "h264-src" {
                h264_src = Some(elem.dynamic_cast::<gst_app::AppSrc>().unwrap());
                break;
            }
        } //gst_app::AppSrc::from_object(pipeline.downcast_ref().unwrap());

        let caps = gst::Caps::from_str(
            "video/x-h264,colorimetry=bt709,stream-format=(string)byte-stream,alignment=(string)au",
        )
        .unwrap();

        let h264_src = h264_src.unwrap();

        h264_src.set_caps(Some(&caps));
        h264_src.set_is_live(true);
        h264_src.set_stream_type(gst_app::AppStreamType::Stream);
        h264_src.set_format(gst::Format::Time);
        h264_src.set_property("emit-signals", true);

        // let caps = Caps::from_str("video/x-h264,colorimetry=bt709,stream-format=(string)byte-stream,alignment=(string)au").unwrap();
        // let h264_appsrc = AppSrc::builder()
        //     .is_live(true)
        //     .stream_type(AppStreamType::Stream)
        //     .caps(&caps)
        //     .format(gst::Format::Time)
        //     .build();

        // let h264parse = gst::ElementFactory::make("h264parse").build().unwrap();
        // let avdec_h264 = gst::ElementFactory::make("avdec_h264").build().unwrap();
        // let videoconvert = gst::ElementFactory::make("videoconvert").build().unwrap();
        // let autovideosink = gst::ElementFactory::make("autovideosink")
        //     // .property("sync", false)
        //     // .property("emit-signals", true)
        //     .build()
        //     .unwrap();

        // h264pipeline
        //     .add_many(&[
        //         h264_appsrc.upcast_ref(),
        //         &h264parse,
        //         &avdec_h264,
        //         &videoconvert,
        //         &autovideosink,
        //     ])
        //     .unwrap();
        // gst::Element::link_many(&[
        //     h264_appsrc.upcast_ref(),
        //     &h264parse,
        //     &avdec_h264,
        //     &videoconvert,
        //     &autovideosink,
        // ])
        //     .unwrap();

        Self {
            alac: (alac_pipeline, alac_appsrc),
            aac_eld: (aac_eld_pipeline, aac_eld_appsrc),
            h264: (h264pipeline, h264_src),
            audio_compression_type: RwLock::new(CompressionType::Alac),
        }
    }
}

impl AirPlayConsumer for VideoConsumer {
    fn on_video(&self, bytes: Vec<u8>) {
        let buffer = gst::Buffer::from_mut_slice(bytes);
        self.h264.1.push_buffer(buffer).ok();
        // log::info!("on_video...");
        // if let Some(child) = self.ffplay.write().unwrap().as_mut() {
        //     if let Some(child_stdin) = child.stdin.as_mut() {
        //         child_stdin.write_all(&bytes).unwrap();
        //         child_stdin.flush().unwrap();
        //     }
        // }
    }

    fn on_video_format(
        &self,
        video_stream_info: airplay2_protocol::airplay::lib::video_stream_info::VideoStreamInfo,
    ) {
        // self.start_ffplay();
        self.h264
            .0
            .set_state(gst::State::Playing)
            .expect("Unable to set the pipeline to the `Playing` state");
        log::info!(
            "OnVideo Format... {:?}",
            video_stream_info.get_stream_connection_id()
        );
    }

    fn on_video_src_disconnect(&self) {
        log::info!("OnVideo Disconnect...");
        self.h264
            .0
            .set_state(gst::State::Null)
            .expect("Unable to set the pipeline to the `Null` state");
    }

    fn on_audio_format(
        &self,
        audio_stream_info: airplay2_protocol::airplay::lib::audio_stream_info::AudioStreamInfo,
    ) {
        log::info!(
            "on_audio_format... type = {:?}",
            audio_stream_info.compression_type
        );
        *self.audio_compression_type.write().unwrap() = audio_stream_info.compression_type;
        self.alac
            .0
            .set_state(gst::State::Playing)
            .expect("Unable to set the pipeline to the `Playing` state");
        self.aac_eld
            .0
            .set_state(gst::State::Playing)
            .expect("Unable to set the pipeline to the `Playing` state");
    }

    fn on_audio(&self, bytes: Vec<u8>) {
        // log::info!("on_audio bytes = {}", bytes.len());
        let buffer = gst::Buffer::from_mut_slice(bytes);
        match *self.audio_compression_type.read().unwrap() {
            CompressionType::Alac => {
                self.alac.1.push_buffer(buffer).ok();
            }
            _ => {
                self.aac_eld.1.push_buffer(buffer).ok();
            }
        }
        // self.tx.send(bytes).unwrap();
        // log::info!("on_audio...");
    }

    fn on_audio_src_disconnect(&self) {
        self.alac
            .0
            .set_state(gst::State::Null)
            .expect("Unable to set the pipeline to the `Null` state");
        self.aac_eld
            .0
            .set_state(gst::State::Null)
            .expect("Unable to set the pipeline to the `Null` state");
    }
}
