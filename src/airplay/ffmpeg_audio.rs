use airplay2_protocol::airplay::server::AudioPacket;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, Device, Sample, SampleRate, Stream, SupportedStreamConfig,
};
use crossbeam::channel::{Receiver, Sender};
use ffmpeg::{
    codec::{Id, Parameters},
    decoder::Audio,
    format, ChannelLayout, Packet,
};
use ffmpeg_next::{self as ffmpeg};
use std::{
    cell::UnsafeCell,
    sync::{atomic::AtomicU64, Arc, Mutex},
};

use crate::{
    audio::sample_rate::SampleRateConverter,
    ffp::{ff_aac_par, ff_alac_par},
};

type PcmSample = i16;

enum AudioFrame {
    Audio(Packet, u32),
    Volume(f32),
    End,
}

struct RingBuffer<T, const BUFFER_LEN: usize> {
    buffer: [T; BUFFER_LEN],
    start: usize,
    end: usize,
}

impl<T: Default + Copy, const BUFFER_LEN: usize> RingBuffer<T, BUFFER_LEN> {
    fn new() -> Self {
        Self {
            buffer: [T::default(); BUFFER_LEN],
            start: 0,
            end: 0,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    #[inline]
    fn push(&mut self, item: T) {
        if self.is_full() {
            return;
        }
        self.buffer[self.end] = item;
        self.end = (self.end + 1) % BUFFER_LEN;
    }

    #[allow(dead_code)]
    fn push_slice(&mut self, item: &[T]) {
        item.iter().for_each(|x| self.push(*x));
    }

    fn pop_slice(&mut self, out: &mut [T]) -> usize {
        let mut count = 0;
        for slot in out {
            if self.is_empty() {
                return 0;
            }
            *slot = self.pop_unchecked();
            count += 1;
        }
        count
    }

    #[inline]
    fn pop_unchecked(&mut self) -> T {
        let item = self.buffer[self.start];
        self.start = (self.start + 1) % BUFFER_LEN;
        item
    }

    #[allow(dead_code)]
    #[inline]
    fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        Some(self.pop_unchecked())
    }

    #[inline]
    fn len(&self) -> usize {
        (self.end + BUFFER_LEN - self.start) % BUFFER_LEN
    }

    #[inline]
    fn is_full(&self) -> bool {
        (self.end + 1) % BUFFER_LEN == self.start
    }
}

type SharedPcmBuffer = Arc<Mutex<RingBuffer<PcmSample, 65536>>>;

struct AudioCpal {
    // cvt: AudioCVT,
    device: Device,
    config: SupportedStreamConfig,
    shared_buffer: SharedPcmBuffer,
}

impl AudioCpal {
    fn new(buffer: SharedPcmBuffer) -> Self {
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
            shared_buffer: buffer, // channel: crossbeam::channel::bounded(32),
        }
    }
}

impl AudioCpal {
    pub fn play(&self) -> anyhow::Result<Stream> {
        let ring_buf = self.shared_buffer.clone();
        let mut config = self.config.config();
        config.buffer_size = BufferSize::Fixed(512);
        let stream = self.device.build_output_stream(
            &config,
            move |data: &mut [PcmSample], _info| {
                let mut buf = ring_buf.lock().unwrap();
                let filled = buf.pop_slice(data);
                data[filled..].fill(Sample::EQUILIBRIUM);
            },
            |err| {
                tracing::error!("stream error {err:?}");
            },
            None,
        )?;
        stream.play()?;
        Ok(stream)
    }
}

pub(super) struct FfMpegAudio {
    decoder: UnsafeCell<Option<Audio>>,
    audio_channel: (Sender<AudioFrame>, Receiver<AudioFrame>),
    samples_per_frame: AtomicU64,
}

impl Default for FfMpegAudio {
    fn default() -> Self {
        Self {
            samples_per_frame: 0.into(),
            decoder: None.into(),
            audio_channel: crossbeam::channel::unbounded(),
        }
    }
}

impl FfMpegAudio {
    pub fn set_samples_per_frame(&self, samples_per_frame: u64) {
        self.samples_per_frame
            .store(samples_per_frame, std::sync::atomic::Ordering::Relaxed);
    }

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
        // let samples_per_frame = self
        //     .samples_per_frame
        //     .load(std::sync::atomic::Ordering::Relaxed) as usize;
        let (max_len, _min_len) = if decoder.codec().unwrap().id() == Id::ALAC {
            (15000, 6000)
        } else {
            (8000, 3000)
        };
        std::thread::spawn(move || {
            let shared_buffer = Arc::new(Mutex::new(RingBuffer::new()));
            let audio_cpal = AudioCpal::new(shared_buffer.clone());
            let sample_rate = audio_cpal.config.sample_rate().0;
            let channels = audio_cpal.config.channels() as u32;
            let mut volume = 0.5;
            if let Ok(_stream) = audio_cpal.play() {
                let mut sample_convert = None;
                let mut audio = ffmpeg::frame::Audio::empty();
                let mut audio_convert_frame = ffmpeg::frame::Audio::empty();
                // let mut pcm_buffer = [0i16; 4096];
                // let mut pcm_buffer_len;
                let mut rate = 44100;
                while let Ok(audio_frame) = rx.recv() {
                    match audio_frame {
                        AudioFrame::Audio(packet, pts) => {
                            match decoder.send_packet(&packet) {
                                Ok(_) => {
                                    if decoder.receive_frame(&mut audio).is_err() {
                                        continue;
                                        // let convert_sample = cvt.convert(audio_convert_frame.data(0).to_vec());
                                    }
                                }
                                Err(err) => {
                                    tracing::error!("audio send packet error! {:?}", err);
                                    continue;
                                }
                            };
                            /* audio.set_pts(Some(pts as i64));
                            let buffer_len = audio_cpal.buffer_len();
                            if buffer_len > max_len && rate < 44704 {
                                rate += 2;
                            } else if rate > 44100 {
                                rate -= 2;
                            }
                            audio.set_rate(rate);
                            let pcm_samples: Vec<i16> = match audio.format() {
                                format::Sample::I16(format::sample::Type::Planar) => {
                                    let channel1 = audio.plane::<i16>(0);
                                    let channel2 = audio.plane::<i16>(1);
                                    pcm_buffer_len = 0;
                                    channel1.iter().zip(channel2).for_each(|(a, b)| {
                                        let a = (*a as f32 * volume) as i16;
                                        let b = (*b as f32 * volume) as i16;
                                        pcm_buffer[pcm_buffer_len..pcm_buffer_len + 2]
                                            .copy_from_slice(&[a, b]);
                                        pcm_buffer_len += 2;
                                    });
                                    let pcm_samples = &pcm_buffer[..pcm_buffer_len];
                                    let convert = SampleRateConverter::new(
                                        pcm_samples.iter().copied(),
                                        SampleRate(audio.rate()),
                                        SampleRate(sample_rate),
                                        2,
                                    );
                                    convert.collect()
                                }
                                _ => {
                                    let mut sample_convert = audio
                                        .resampler(
                                            format::Sample::I16(format::sample::Type::Packed),
                                            ChannelLayout::STEREO,
                                            audio.rate(),
                                        )
                                        .unwrap();
                                    sample_convert
                                        .run(&audio, &mut audio_convert_frame)
                                        .unwrap();
                                    let pcm_samples =
                                        audio_convert_frame.data(0).chunks(2).map(|buf| {
                                            (i16::from_le_bytes(buf.try_into().unwrap()) as f32
                                                * volume)
                                                as i16
                                        });
                                    let convert = SampleRateConverter::new(
                                        pcm_samples,
                                        SampleRate(audio.rate()),
                                        SampleRate(sample_rate),
                                        2,
                                    );
                                    convert.collect()
                                }
                            }; */

                            let sample_convert = if let Some(sc) = &mut sample_convert {
                                sc
                            } else {
                                sample_convert = audio
                                    .resampler(
                                        format::Sample::I16(format::sample::Type::Packed),
                                        ChannelLayout::STEREO,
                                        audio.rate(),
                                    )
                                    .ok();
                                sample_convert.as_mut().unwrap()
                            };
                            sample_convert
                                .run(&audio, &mut audio_convert_frame)
                                .unwrap();
                            audio_convert_frame.set_pts(Some(pts as i64));
                            let buffer_len = shared_buffer.lock().unwrap().len();
                            if buffer_len > max_len {
                                if rate < 44704 {
                                    rate += channels;
                                    // tracing::info!("采样率提高 {}", rate);
                                }
                            } else if rate > 44100 {
                                rate -= channels;
                                // tracing::info!("采样率降低 {}", rate);
                            }
                            audio_convert_frame.set_rate(rate);
                            let pcm_samples =
                                audio_convert_frame.data(0).chunks_exact(2).map(|buf| {
                                    (PcmSample::from_le_bytes(buf.try_into().unwrap()) as f32
                                        * volume) as PcmSample
                                });
                            let convert = SampleRateConverter::new(
                                pcm_samples,
                                SampleRate(audio_convert_frame.rate()),
                                SampleRate(sample_rate),
                                channels as u16,
                            );
                            let mut buffer = shared_buffer.lock().unwrap();
                            for v in convert {
                                if buffer.is_full() {
                                    tracing::warn!("超出缓冲区大小..");
                                    break;
                                }
                                buffer.push(v);
                            }
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
            tracing::info!("Stop Cpal Audio...");
        });
    }

    pub fn push_buffer(&self, buf: &AudioPacket) -> anyhow::Result<()> {
        let packet = Packet::copy(buf.audio_buf());
        self.audio_channel
            .0
            .send(AudioFrame::Audio(packet, buf.timestamp()))?;
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
        let i = i * 2;
        let item = &hex[i..i + 2];
        extra_data.push(u8::from_str_radix(item, 16).unwrap());
    }
    extra_data
}
