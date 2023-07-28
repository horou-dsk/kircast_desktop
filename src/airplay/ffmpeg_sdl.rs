use std::{
    cell::UnsafeCell,
    sync::mpsc::{self, Sender, TryRecvError},
    time::Duration,
};

use ffmpeg::{format::Pixel, Packet};
use ffmpeg_next::{self as ffmpeg, codec::Id};
use sdl2::{
    event::Event,
    keyboard::Keycode,
    pixels::{Color, PixelFormatEnum},
    rect::Rect,
    render::TextureAccess,
};

pub(super) struct SdlFfmpeg {
    width: u32,
    height: u32,
    video_decoder: UnsafeCell<ffmpeg::decoder::Video>,
    tx: UnsafeCell<Option<Sender<ffmpeg::frame::Video>>>,
}

impl SdlFfmpeg {
    pub fn new(width: u32, height: u32) -> Self {
        let codec = ffmpeg::codec::decoder::find(Id::H264).unwrap();
        let decoder = ffmpeg::decoder::new()
            .open_as(codec)
            .unwrap()
            .video()
            .unwrap();
        Self {
            width,
            height,
            video_decoder: UnsafeCell::new(decoder),
            tx: UnsafeCell::new(None),
        }
    }

    pub fn start(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel::<ffmpeg::frame::Video>();

        let width = self.width;
        let height = self.height;
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
                match rx.try_recv() {
                    Ok(frame) => {
                        canvas
                            .with_texture_canvas(&mut texture, |texture_canvas| {
                                texture_canvas.set_draw_color(Color::RGB(0, 0, 0));
                                texture_canvas.clear();
                            })
                            .expect("clear texture error!");
                        texture
                            .update(
                                Rect::new(
                                    ((width - frame.width()) / 2) as i32,
                                    ((height - frame.height()) / 2) as i32,
                                    frame.width(),
                                    frame.height(),
                                ),
                                frame.data(0),
                                frame.stride(0),
                            )
                            .unwrap();
                        canvas.copy(&texture, None, None).unwrap();
                        canvas.present();
                    }
                    Err(TryRecvError::Empty) => (),
                    Err(TryRecvError::Disconnected) => {
                        break 'running;
                    }
                }
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
                ::std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 144));
            }
        });

        unsafe {
            (*self.tx.get()).replace(tx);
        }

        Ok(())
    }

    pub fn push_buffer(&self, buf: &[u8]) -> anyhow::Result<()> {
        let video_decoder = unsafe { &mut *self.video_decoder.get() };
        let packet = Packet::copy(buf);
        let mut frame = ffmpeg::frame::Video::empty();
        video_decoder.send_packet(&packet)?;
        if video_decoder.receive_frame(&mut frame).is_ok() {
            if let Some(tx) = unsafe { (*self.tx.get()).as_ref() } {
                let mut scaler = ffmpeg::software::converter(
                    (frame.width(), frame.height()),
                    frame.format(),
                    Pixel::RGB24,
                )
                .unwrap();
                let mut rgb_frame = ffmpeg::frame::Video::empty();
                scaler.run(&frame, &mut rgb_frame).unwrap();
                if tx.send(rgb_frame).is_err() {
                    self.stop();
                }
            }
        }
        Ok(())
    }

    pub fn stop(&self) {
        unsafe {
            (*self.tx.get()).take();
        }
    }
}
