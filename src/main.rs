mod dialog;
mod gameboy;
mod menu;

use std::{
    fs,
    ptr::addr_of_mut,
    time::{Duration, Instant},
    sync::{Arc, Mutex},
};

use sdl2::{
    audio::AudioSpecDesired,
    messagebox::MessageBoxFlag,
    pixels::{Color, PixelFormatEnum},
    rect::{Point, Rect},
    render::{TextureCreator, TextureQuery, WindowCanvas},
    ttf::Font,
    video::Window,
    video::WindowContext,
    VideoSubsystem,
    audio::{AudioCallback, AudioDevice, AudioStatus},
};
use tao::{
    dpi::PhysicalSize,
    event::{DeviceEvent, Event, WindowEvent},
    event::{ElementState, RawKeyEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::KeyCode,
    platform::run_return::EventLoopExtRunReturn,
    platform::windows::WindowExtWindows,
    window::WindowBuilder,
    menu::MenuId,
};
use crate::{
    gameboy::{Buttons, GameBoy},
    menu::MENU_OPEN,
};

#[macro_use]
extern crate bitflags;

const FRAME_DURATION: Duration = Duration::from_micros(16_742);

const COLORS: [Color; 4] = [
    Color::RGB(0xff, 0xff, 0xff),
    Color::RGB(0xc0, 0xc0, 0xc0),
    Color::RGB(0x40, 0x40, 0x40),
    Color::RGB(0, 0, 0),
];
// const COLORS: [Color; 4] = [
//     Color::RGB(0xe2, 0xf3, 0xe4),
//     Color::RGB(0x94, 0xe3, 0x44),
//     Color::RGB(0x46, 0x87, 0x8f),
//     Color::RGB(0x33, 0x2c, 0x50),
// ];

struct Callback {
    gameboy: Arc<Mutex<GameBoy>>,
}

impl AudioCallback for Callback {
    type Channel = f32;

    fn callback(&mut self, buffer: &mut [Self::Channel]) {
        let mut gameboy = self.gameboy.lock().unwrap();

        while gameboy.audio_buffer_size() < gameboy::apu::AUDIO_BUFFER_SIZE {
            gameboy.tick();
        }

        buffer.copy_from_slice(gameboy.extract_audio_buffer().as_slice());
    }
}

struct Context {
    pub audio_device: AudioDevice<Callback>,
}

fn main() -> Result<(), String> {
    if let Err(msg) = run() {
        sdl2::messagebox::show_simple_message_box(MessageBoxFlag::ERROR, "YAGBE", &msg, None)
            .map_err(|err| err.to_string())?;

        return Err(msg);
    }

    Ok(())
}

fn run() -> Result<(), String> {
    let gameboy = GameBoy::new();
    let gameboy = Arc::new(Mutex::new(gameboy));

    // Window
    let mut event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Yet Another Game Boy Emulator")
        .with_menu(menu::build_menu())
        .with_inner_size(PhysicalSize::new(320, 288 + menu_height()))
        .with_resizable(false)
        .build(&event_loop)
        .map_err(|e| e.to_string())?;

    // SDL
    let sdl_context = sdl2::init()?;

    // Load a font
    let ttf_context = sdl2::ttf::init().map_err(|e| e.to_string())?;
    let font = ttf_context.load_font("JetBrainsMono-Regular.ttf", 9)?;

    // Video
    let video_subsystem = sdl_context.video()?;

    let sdl_window = init_sdl_window(&window, video_subsystem);

    let mut canvas = sdl_window.into_canvas().build().map_err(|e| e.to_string())?;

    let texture_creator = canvas.texture_creator();

    let mut screen = texture_creator
        .create_texture_streaming(PixelFormatEnum::RGB24, gameboy::SCREEN_WIDTH as u32, gameboy::SCREEN_HEIGHT as u32)
        .map_err(|e| e.to_string())?;

    // Audio
    let desired_spec = AudioSpecDesired {
        freq: Some(gameboy::apu::AUDIO_SAMPLE_RATE as i32),
        channels: Some(2),
        samples: Some(gameboy::apu::AUDIO_BUFFER_SIZE as u16 / 2),
    };

    let audio_subsystem = sdl_context.audio()?;
    let audio_device = audio_subsystem.audio_playback_device_name(0)?;
    let device = audio_subsystem.open_playback(audio_device.as_str(), &desired_spec, |_spec| {
        Callback {
            gameboy: gameboy.clone()
        }
    })?;

    let context = Context {
        audio_device: device,
    };

    if let Some(rom_path) = std::env::args().nth(1) {
        let rom = fs::read(rom_path).map_err(|_| "Could not read ROM file")?;

        gameboy.lock().unwrap().load(rom);

        context.audio_device.resume();
    }

    let mut show_fps = false;

    let mut frame_start = Instant::now();
    let mut frame_delta = FRAME_DURATION;

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::DeviceEvent { event: DeviceEvent::Key(RawKeyEvent { physical_key, state: ElementState::Pressed }), .. } => gameboy.lock().map(|mut gameboy| {
                match physical_key {
                    KeyCode::F2 => show_fps = !show_fps,

                    KeyCode::ArrowDown => gameboy.button_pressed(Buttons::Down),
                    KeyCode::ArrowUp => gameboy.button_pressed(Buttons::Up),
                    KeyCode::ArrowLeft => gameboy.button_pressed(Buttons::Left),
                    KeyCode::ArrowRight => gameboy.button_pressed(Buttons::Right),

                    KeyCode::Enter => gameboy.button_pressed(Buttons::Start),
                    KeyCode::Tab => gameboy.button_pressed(Buttons::Select),
                    KeyCode::AltLeft => gameboy.button_pressed(Buttons::A),
                    KeyCode::ControlLeft => gameboy.button_pressed(Buttons::B),
                    _ => {}
                }
            }).unwrap(),
            Event::DeviceEvent { event: DeviceEvent::Key(RawKeyEvent { physical_key, state: ElementState::Released }), .. } => gameboy.lock().map(|mut gameboy| {
                match physical_key {
                    KeyCode::ArrowDown => gameboy.button_released(Buttons::Down),
                    KeyCode::ArrowUp => gameboy.button_released(Buttons::Up),
                    KeyCode::ArrowLeft => gameboy.button_released(Buttons::Left),
                    KeyCode::ArrowRight => gameboy.button_released(Buttons::Right),

                    KeyCode::Enter => gameboy.button_released(Buttons::Start),
                    KeyCode::Tab => gameboy.button_released(Buttons::Select),
                    KeyCode::AltLeft => gameboy.button_released(Buttons::A),
                    KeyCode::ControlLeft => gameboy.button_released(Buttons::B),
                    _ => {}
                }
            }).unwrap(),
            Event::MenuEvent { menu_id, .. } => gameboy.lock()
                .map(|mut gameboy| handle_menu_event(&mut gameboy, &context, menu_id))
                .unwrap(),
            Event::MainEventsCleared => {
                // TODO: Wait until a screen is ready to draw.
                window.request_redraw();
            }
            Event::RedrawRequested(_) => gameboy.lock().map(|gameboy| {
                frame_start = Instant::now();

                // Draw screen
                {
                    screen.with_lock(None, |buffer: &mut [u8], pitch: usize| {
                        for (index, &color) in gameboy.screen().iter().enumerate() {
                            let x = index % gameboy::SCREEN_WIDTH;
                            let y = index / gameboy::SCREEN_WIDTH;

                            let color = COLORS[color as usize];

                            let offset = y * pitch + x * 3;
                            buffer[offset] = color.r;
                            buffer[offset + 1] = color.g;
                            buffer[offset + 2] = color.b;
                        }
                    }).unwrap();

                    // Draw screen
                    canvas.copy(&screen, None, Some(Rect::new(0, 0, (gameboy::SCREEN_WIDTH * 2) as u32, (gameboy::SCREEN_HEIGHT * 2) as u32))).unwrap();

                    if show_fps {
                        render_text(&font, &mut canvas, &texture_creator, format!("{:.2}", 1.0 / frame_delta.as_secs_f32()).as_str(), Point::new(4, 4)).unwrap();
                    }

                    canvas.present();
                }

                frame_delta = frame_start.elapsed();

                frame_start = Instant::now();
            }).unwrap(),
            _ => {}
        };
    });

    Ok(())
}

fn handle_menu_event(mut gameboy: &mut GameBoy, context: &Context, menu_id: MenuId) {
    match menu_id {
        MENU_OPEN => {
            open_rom(&mut gameboy).unwrap();

            if context.audio_device.status() != AudioStatus::Playing {
                context.audio_device.resume();
            }
        }
        _ => {}
    }
}

fn menu_height() -> i32 {
    use windows::{
        Win32::Foundation::{RECT},
        Win32::UI::WindowsAndMessaging::{AdjustWindowRectEx, WINDOW_EX_STYLE, WINDOW_STYLE},
    };

    let mut rect = RECT::default();

    unsafe {
        AdjustWindowRectEx(addr_of_mut!(rect), WINDOW_STYLE::default(), true, WINDOW_EX_STYLE::default());
    }

    rect.top.abs()
}

fn init_sdl_window(window: &tao::window::Window, video_subsystem: VideoSubsystem) -> Window {
    unsafe {
        let sdl_window = sdl2::sys::SDL_CreateWindowFrom(window.hwnd());

        Window::from_ll(video_subsystem.clone(), sdl_window)
    }
}

fn open_rom(gameboy: &mut GameBoy) -> Result<(), String> {
    if let Ok(rom_path) = dialog::open_file() {
        let rom = fs::read(rom_path).map_err(|_| "Could not read ROM file")?;

        gameboy.load(rom);
    }

    Ok(())
}

fn render_text(font: &Font, canvas: &mut WindowCanvas, texture_creator: &TextureCreator<WindowContext>, text: &str, pos: Point) -> Result<(), String> {
    // render a surface, and convert it to a texture bound to the canvas
    let surface = font
        .render(text)
        .shaded(Color::RGBA(255, 255, 0, 255), Color::RGBA(0, 0, 0, 192))
        .map_err(|e| e.to_string())?;
    let texture = texture_creator
        .create_texture_from_surface(&surface)
        .map_err(|e| e.to_string())?;

    let TextureQuery { width, height, .. } = texture.query();

    // Draw FPS
    canvas.copy(&texture, None, Some(Rect::new(pos.x, pos.y, width, height)))?;

    Ok(())
}
