use std::{
    cell::UnsafeCell,
    sync::{
        atomic::AtomicBool,
        mpsc::{self, Sender},
        Arc,
    },
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
    tx: UnsafeCell<Option<Sender<Packet>>>,
}

struct VideoFrame(UnsafeCell<Option<ffmpeg::frame::Video>>);

unsafe impl Sync for VideoFrame {}
unsafe impl Send for VideoFrame {}

impl SdlFfmpeg {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            tx: UnsafeCell::new(None),
        }
    }

    pub fn start(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel::<Packet>();

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

            let codec = ffmpeg::codec::decoder::find(Id::H264).unwrap();
            let mut decoder = ffmpeg::decoder::new()
                .open_as(codec)
                .unwrap()
                .video()
                .unwrap();

            let is_close = Arc::new(AtomicBool::new(false));
            let frame = Arc::new(VideoFrame(UnsafeCell::new(None)));

            {
                let root_frame = frame.clone();
                let is_close = is_close.clone();
                std::thread::spawn(move || {
                    while let Ok(packet) = rx.recv() {
                        if let Err(err) = decoder.send_packet(&packet) {
                            log::error!("send packet error! {:?}", err);
                        } else {
                            let mut frame = ffmpeg::frame::Video::empty();
                            while decoder.receive_frame(&mut frame).is_ok() {
                                let mut scaler = ffmpeg::software::converter(
                                    (frame.width(), frame.height()),
                                    frame.format(),
                                    Pixel::RGB24,
                                )
                                .unwrap();
                                let mut rgb_frame = ffmpeg::frame::Video::empty();
                                scaler.run(&frame, &mut rgb_frame).unwrap();
                                unsafe {
                                    (*root_frame.0.get()).replace(rgb_frame);
                                }
                            }
                        }
                    }
                    is_close.store(true, std::sync::atomic::Ordering::Relaxed);
                });
            }

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
                if is_close.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                if let Some(rgb_frame) = unsafe { (*frame.0.get()).take() } {
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
                ::std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 144));
            }
        });

        unsafe {
            (*self.tx.get()).replace(tx);
        }

        Ok(())
    }

    pub fn push_buffer(&self, buf: &[u8]) -> anyhow::Result<()> {
        let packet = Packet::copy(buf);
        if let Some(tx) = unsafe { (*self.tx.get()).as_ref() } {
            if tx.send(packet).is_err() {
                self.stop();
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
