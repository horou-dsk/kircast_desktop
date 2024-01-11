use std::cell::UnsafeCell;

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Sample, SampleRate, Stream, SupportedStreamConfig,
};
use crossbeam::channel::{Receiver, Sender};
use ffmpeg::{
    codec::{Id, Parameters},
    decoder::Audio,
    format, ChannelLayout, Packet,
};
use ffmpeg_next::{self as ffmpeg};

use crate::{
    audio::sample_rate::SampleRateConverter,
    ffp::{ff_aac_par, ff_alac_par},
};

type PcmSample = i16;

enum AudioFrame {
    Audio(Packet),
    Volume(f32),
    End,
}

struct AudioCpal {
    // cvt: AudioCVT,
    device: Device,
    config: SupportedStreamConfig,
    channel: (Sender<Vec<PcmSample>>, Receiver<Vec<PcmSample>>),
}

impl Default for AudioCpal {
    fn default() -> Self {
        let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        let mut supported_configs_range = device.supported_output_configs().unwrap();
        let config = supported_configs_range
            .next()
            .unwrap()
            .with_max_sample_rate();
        Self {
            device,
            config,
            channel: crossbeam::channel::bounded(32),
        }
    }
}

impl AudioCpal {
    pub fn play(&self) -> anyhow::Result<Stream> {
        let mut frame_buf: Vec<PcmSample> = Vec::new();
        let rx = self.channel.1.clone();
        let stream = self.device.build_output_stream(
            &self.config.config(),
            move |data: &mut [PcmSample], _info| {
                while let Ok(buf) = rx.try_recv() {
                    frame_buf.extend(buf);
                }
                // log::info!(
                //     "frame_buf len = {} data len = {} info = {:?}",
                //     frame_buf.len(),
                //     data.len(),
                //     _info
                // );
                if frame_buf.len() >= data.len() {
                    frame_buf.drain(..data.len()).zip(data).for_each(|(f, t)| {
                        *t = f;
                    });
                } else {
                    let mut frame_buf = frame_buf.drain(..);
                    data.iter_mut()
                        .for_each(|v| *v = frame_buf.next().unwrap_or(Sample::EQUILIBRIUM));
                }
            },
            |err| {
                log::error!("stream error {err:?}");
            },
            None,
        )?;
        stream.play()?;
        Ok(stream)
    }

    pub fn push_buffer(&self, buf: Vec<PcmSample>) -> anyhow::Result<()> {
        self.channel.0.send(buf)?;
        Ok(())
    }
}

pub(super) struct FfMpegAudio {
    decoder: UnsafeCell<Option<Audio>>,
    audio_channel: (Sender<AudioFrame>, Receiver<AudioFrame>),
}

impl Default for FfMpegAudio {
    fn default() -> Self {
        Self {
            decoder: None.into(),
            audio_channel: crossbeam::channel::unbounded(),
        }
    }
}

impl FfMpegAudio {
    pub fn start_alac(&self) -> anyhow::Result<()> {
        let codec = ffmpeg::codec::decoder::find(Id::ALAC).unwrap();
        let mut ctx = ffmpeg::decoder::new();
        let buffer = "00000024616c616300000000000001600010280a0e0200ff00000000000000000000ac44";
        let extra_data = hex_to_buf(buffer);
        let par =
            unsafe { Parameters::wrap(ff_alac_par(extra_data.as_ptr(), extra_data.len()), None) };
        ctx.set_parameters(par)?;
        let decoder = ctx.open_as(codec)?.audio()?;
        self.play_audio(decoder);
        Ok(())
    }

    pub fn start_aac(&self) -> anyhow::Result<()> {
        let codec = ffmpeg::codec::decoder::find(Id::AAC).unwrap();
        let extra_data = hex_to_buf("f8e85000");
        let par =
            unsafe { Parameters::wrap(ff_aac_par(extra_data.as_ptr(), extra_data.len()), None) };
        let mut ctx = ffmpeg::decoder::new();
        ctx.set_parameters(par)?;
        let decoder = ctx.open_as(codec)?.audio()?;
        self.play_audio(decoder);
        Ok(())
    }

    pub fn stop(&self) {
        self.audio_channel.0.send(AudioFrame::End).unwrap();
        unsafe {
            (*self.decoder.get()).take();
        }
    }

    fn play_audio(&self, mut decoder: Audio) {
        let rx = self.audio_channel.1.clone();
        std::thread::spawn(move || {
            let audio_cpal = AudioCpal::default();
            let sample_rate = audio_cpal.config.sample_rate().0;
            let mut volume = 0.5;
            if let Ok(_stream) = audio_cpal.play() {
                let mut sample_convert = None;
                while let Ok(audio_frame) = rx.recv() {
                    match audio_frame {
                        AudioFrame::Audio(packet) => {
                            let audio = match decoder.send_packet(&packet) {
                                Ok(_) => {
                                    let mut audio = ffmpeg::frame::Audio::empty();
                                    if decoder.receive_frame(&mut audio).is_ok() {
                                        audio
                                        // let convert_sample = cvt.convert(audio_convert_frame.data(0).to_vec());
                                    } else {
                                        continue;
                                    }
                                }
                                Err(err) => {
                                    log::error!("audio send packet error! {:?}", err);
                                    continue;
                                }
                            };

                            let sample_convert = if let Some(sc) = &mut sample_convert {
                                sc
                            } else {
                                sample_convert = ffmpeg::software::resampler(
                                    (audio.format(), audio.channel_layout(), audio.rate()),
                                    (
                                        format::Sample::I16(format::sample::Type::Packed),
                                        ChannelLayout::STEREO,
                                        audio.rate(),
                                    ),
                                )
                                .ok();
                                sample_convert.as_mut().unwrap()
                            };
                            let mut audio_convert_frame = ffmpeg::frame::Audio::empty();
                            sample_convert
                                .run(&audio, &mut audio_convert_frame)
                                .unwrap();
                            let pcm_samples = audio_convert_frame.data(0).chunks(2).map(|buf| {
                                (i16::from_le_bytes(buf.try_into().unwrap()) as f32 * volume) as i16
                            });
                            let convert = SampleRateConverter::new(
                                pcm_samples,
                                SampleRate(audio_convert_frame.rate()),
                                SampleRate(sample_rate),
                                2,
                            );
                            // let pcm_samples = convert
                            //     .map(|v| PcmSample::from_sample(v) * volume)
                            //     .collect();
                            audio_cpal.push_buffer(convert.collect()).unwrap();
                        }
                        AudioFrame::End => {
                            break;
                        }
                        AudioFrame::Volume(vol) => {
                            volume = vol;
                        }
                    }
                }
            }
            log::info!("Stop Cpal Audio...");
        });
    }

    pub fn push_buffer(&self, buf: &[u8]) -> anyhow::Result<()> {
        let packet = Packet::copy(buf);
        self.audio_channel.0.send(AudioFrame::Audio(packet))?;
        Ok(())
    }

    pub fn set_volume(&self, vol: f32) -> anyhow::Result<()> {
        self.audio_channel.0.send(AudioFrame::Volume(vol))?;
        Ok(())
    }
}

fn hex_to_buf(hex: &str) -> Vec<u8> {
    let mut extra_data = Vec::with_capacity(hex.len() / 2);
    for i in 0..hex.len() / 2 {
        let item = &hex[i * 2..i * 2 + 2];
        extra_data.push(u8::from_str_radix(item, 16).unwrap());
    }
    extra_data
}
