use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crossbeam::channel::{Receiver, Sender, TryRecvError};
use ffmpeg::{format::Pixel, Packet};
use ffmpeg_next::{self as ffmpeg, codec::Id};
use sdl2::{
    event::Event,
    keyboard::Keycode,
    pixels::{Color, PixelFormatEnum},
    rect::Rect,
    render::TextureAccess,
};

enum Frame {
    Pakcet(Packet),
    End,
}

pub(super) struct SdlFfmpeg {
    width: u32,
    height: u32,
    video_packet_channel: (Sender<Frame>, Receiver<Frame>),
    video: Arc<Mutex<ffmpeg::frame::Video>>,
}

impl SdlFfmpeg {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            video_packet_channel: crossbeam::channel::unbounded(),
            video: Arc::new(Mutex::new(ffmpeg::frame::Video::empty())),
        }
    }

    fn create_video_decoder(&self, tx: Sender<()>) {
        let rx = self.video_packet_channel.1.clone();
        let out_video = self.video.clone();
        std::thread::spawn(move || {
            let codec = ffmpeg::codec::decoder::find(Id::H264).unwrap();
            let mut decoder = ffmpeg::decoder::new()
                .open_as(codec)
                .unwrap()
                .video()
                .unwrap();
            let mut wscaler = None;
            let mut video_frame = ffmpeg::frame::Video::empty();
            while let Ok(frame) = rx.recv() {
                match frame {
                    Frame::Pakcet(packet) => {
                        if let Err(err) = decoder.send_packet(&packet) {
                            tracing::error!("send packet error! {:?}", err);
                            video_frame = ffmpeg::frame::Video::empty();
                        } else {
                            while decoder.receive_frame(&mut video_frame).is_ok() {
                                let scaler = match &mut wscaler {
                                    Some(scaler) => scaler,
                                    _ => {
                                        wscaler =
                                            Some(video_frame.converter(Pixel::RGB24).unwrap());
                                        wscaler.as_mut().unwrap()
                                    }
                                };
                                let mut rgb_frame = out_video.lock().unwrap();
                                if scaler.run(&video_frame, &mut rgb_frame).is_err() {
                                    *rgb_frame = ffmpeg::frame::Video::empty();
                                    wscaler = Some(video_frame.converter(Pixel::RGB24).unwrap());
                                } else {
                                    tx.send(()).unwrap();
                                };
                            }
                        }
                    }
                    Frame::End => {
                        break;
                    }
                }
            }
        });
    }

    pub fn start(&self) -> Result<(), String> {
        let width = self.width;
        let height = self.height;
        let (tx, rx) = crossbeam::channel::unbounded();
        self.create_video_decoder(tx);
        let update_video = self.video.clone();
        std::thread::spawn(move || {
            let sdl_context = sdl2::init().expect("sdl init error");
            let video_subsystem = sdl_context.video().expect("sdl video error");
            let window = video_subsystem
                .window("airplay", width, height)
                .position_centered()
                .build()
                .unwrap();

            let mut canvas = window.into_canvas().build().unwrap();
            let texture_creator = canvas.texture_creator();

            let mut texture = texture_creator
                .create_texture(PixelFormatEnum::RGB24, TextureAccess::Target, width, height)
                .unwrap();
            let mut event_pump = sdl_context.event_pump().unwrap();

            'running: loop {
                for event in event_pump.poll_iter() {
                    match event {
                        Event::Quit { .. }
                        | Event::KeyDown {
                            keycode: Some(Keycode::Escape),
                            ..
                        } => break 'running,
                        _ => {}
                    }
                }
                match rx.try_recv() {
                    Ok(()) => {
                        let rgb_frame = update_video.lock().unwrap();
                        if !unsafe { rgb_frame.is_empty() } {
                            canvas
                                .with_texture_canvas(&mut texture, |texture_canvas| {
                                    texture_canvas.set_draw_color(Color::RGB(0, 0, 0));
                                    texture_canvas.clear();
                                })
                                .expect("clear texture error!");
                            texture
                                .update(
                                    Rect::new(
                                        ((width - rgb_frame.width()) / 2) as i32,
                                        ((height - rgb_frame.height()) / 2) as i32,
                                        rgb_frame.width(),
                                        rgb_frame.height(),
                                    ),
                                    rgb_frame.data(0),
                                    rgb_frame.stride(0),
                                )
                                .unwrap();
                            canvas.copy(&texture, None, None).unwrap();
                            canvas.present();
                        }
                    }
                    Err(TryRecvError::Disconnected) => {
                        break;
                    }
                    _ => (),
                }
                ::std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60));
            }
        });

        Ok(())
    }

    pub fn push_buffer(&self, buf: &[u8]) -> anyhow::Result<()> {
        // TODO: 主动退出sdl窗口，会导致消息一直积累
        let packet = Packet::copy(buf);
        if self
            .video_packet_channel
            .0
            .send(Frame::Pakcet(packet))
            .is_err()
        {
            self.stop();
        }
        Ok(())
    }

    pub fn stop(&self) {
        self.video_packet_channel.0.send(Frame::End).unwrap();
    }
}
