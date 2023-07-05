mod dialog;
mod gameboy;
mod menu;

extern crate sdl2;
extern crate raw_window_handle;
extern crate windows;

use std::{
    fs,
    ptr::addr_of_mut,
    time::{Duration, Instant},
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
};
use tao::{
    dpi::PhysicalSize,
    event::{DeviceEvent, Event, WindowEvent},
    event::{ElementState, RawKeyEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::KeyCode,
    menu::{MenuBar, MenuId, MenuItem, MenuItemAttributes},
    platform::run_return::EventLoopExtRunReturn,
    platform::windows::WindowExtWindows,
    window::WindowBuilder,
};
use crate::gameboy::{Buttons, GameBoy};

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

const MENU_OPEN: MenuId = MenuId(1);

fn main() -> Result<(), String> {
    if let Err(msg) = run() {
        sdl2::messagebox::show_simple_message_box(MessageBoxFlag::ERROR, "YAGBE", &msg, None)
            .map_err(|err| err.to_string())?;

        return Err(msg);
    }

    Ok(())
}

fn run() -> Result<(), String> {
    let menu_height = unsafe {
        use windows::{
            Win32::Foundation::{RECT},
            Win32::UI::WindowsAndMessaging::{AdjustWindowRectEx, WINDOW_EX_STYLE, WINDOW_STYLE},
        };

        let mut rect = RECT::default();

        AdjustWindowRectEx(addr_of_mut!(rect), WINDOW_STYLE::default(), true, WINDOW_EX_STYLE::default());

        rect.top.abs()
    };

    let mut event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Yet Another Game Boy Emulator")
        .with_menu(menu::build_menu())
        .with_inner_size(PhysicalSize::new(320, 288 + menu_height))
        .with_resizable(false)
        .build(&event_loop)
        .map_err(|e| e.to_string())?;

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    let sdl_window = unsafe {
        let sdl_window = sdl2::sys::SDL_CreateWindowFrom(window.hwnd());

        Window::from_ll(video_subsystem.clone(), sdl_window)
    };

    let mut gameboy = gameboy::GameBoy::new();

    if let Some(rom_path) = std::env::args().nth(1) {
        let rom = fs::read(rom_path).map_err(|_| "Could not read ROM file")?;

        gameboy.load(rom);
    }

    let audio_subsystem = sdl_context.audio()?;
    let ttf_context = sdl2::ttf::init().map_err(|e| e.to_string())?;

    // Load a font
    let font = ttf_context.load_font("JetBrainsMono-Regular.ttf", 9)?;

    let desired_spec = AudioSpecDesired {
        freq: Some(48_000),
        channels: Some(2),
        samples: None, // default sample size
    };

    let device = audio_subsystem.open_queue::<f32, _>(None, &desired_spec)?;
    device.resume();

    let mut canvas = sdl_window.into_canvas().build().map_err(|e| e.to_string())?;
    let texture_creator = canvas.texture_creator();

    let mut screen = texture_creator
        .create_texture_streaming(PixelFormatEnum::RGB24, 160, 144)
        .map_err(|e| e.to_string())?;

    let mut now = Instant::now();
    let mut sleep_overhead = Duration::ZERO;

    let mut frame_delta = Duration::from_millis(16);

    let mut show_fps = false;

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        let mut time_budget = FRAME_DURATION;

        let previous_now = now;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::DeviceEvent { event: DeviceEvent::Key(RawKeyEvent { physical_key, state: ElementState::Pressed }), .. } =>
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
                },
            Event::DeviceEvent { event: DeviceEvent::Key(RawKeyEvent { physical_key, state: ElementState::Released }), .. } =>
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
                },
            Event::MenuEvent { menu_id, .. } => match menu_id {
                MENU_OPEN => {
                    open_rom(&mut gameboy).unwrap();
                },
                _ => {}
            }
            Event::MainEventsCleared => {
                // Application update code.
                if gameboy.run_to_frame(time_budget) {
                    window.request_redraw();
                }

                let samples = gameboy.extract_audio_buffer();
                if samples.len() > 0 {
                    device.queue_audio(samples.as_slice()).unwrap();
                }
            }
            Event::RedrawRequested(_) => {
                screen.with_lock(None, |buffer: &mut [u8], pitch: usize| {
                    for (index, &color) in gameboy.screen().iter().enumerate() {
                        let x = index % 160;
                        let y = index / 160;

                        let color = COLORS[color as usize];

                        let offset = y * pitch + x * 3;
                        buffer[offset] = color.r;
                        buffer[offset + 1] = color.g;
                        buffer[offset + 2] = color.b;
                    }
                }).unwrap();

                // Draw screen
                canvas.copy(&screen, None, Some(Rect::new(0, 0, 160 * 2, 144 * 2))).unwrap();

                if show_fps {
                    render_text(&font, &mut canvas, &texture_creator, format!("{:.2}", 1.0 / frame_delta.as_secs_f32()).as_str(), Point::new(4, 4)).unwrap();
                }

                canvas.present();

                time_budget = time_budget.saturating_sub(previous_now.elapsed());

                // Take off the delta to compensate for a previous long frame.
                time_budget = time_budget.saturating_sub(sleep_overhead);

                if !time_budget.is_zero() {
                    let before_sleep = Instant::now();

                    std::thread::sleep(time_budget);

                    sleep_overhead = before_sleep.elapsed() - time_budget;
                } else {
                    // Slower than real time. Skip frames?
                }

                frame_delta = now - previous_now;
            }
            _ => {}
        };

        now = Instant::now();
    });

    Ok(())
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
