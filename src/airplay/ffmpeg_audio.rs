use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device, SupportedStreamConfig,
};
use ffmpeg::codec::{Id, Parameters};
use ffmpeg_next::{self as ffmpeg};
use sdl2::audio::{AudioCVT, AudioFormat};

use crate::ffp::ff_alac_par;

struct AudioCpal {
    cvt: AudioCVT,
    device: Device,
    config: SupportedStreamConfig,
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
            cvt: AudioCVT::new(
                AudioFormat::S16LSB,
                2,
                44100,
                AudioFormat::U8,
                config.channels() as u8,
                config.sample_rate().0 as i32,
            )
            .expect("Could not create convert"),
            device,
            config,
        }
    }
}

pub(super) struct FfMpegAudio {}

impl Default for FfMpegAudio {
    fn default() -> Self {
        Self {}
    }
}

impl FfMpegAudio {
    fn create_audio_cpal() -> AudioCpal {
        AudioCpal::default()
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
        Ok(())
    }

    pub fn push_buffer(&self, buf: &[u8]) {}
}

fn hex_to_buf(hex: &str) -> Vec<u8> {
    let mut extra_data = Vec::with_capacity(hex.len() / 2);
    for i in 0..hex.len() / 2 {
        let item = &hex[i * 2..i * 2 + 2];
        extra_data.push(u8::from_str_radix(item, 16).unwrap());
    }
    extra_data
}
