mod cpu;
mod bus;
mod ppu;
mod io_registers;
mod cpu_registers;
mod apu;
mod pixel_fetcher;

extern crate sdl2;

use std::fs;
use std::time::{Duration, Instant};
use std::io::Write;
use sdl2::{event::Event, keyboard::Keycode, pixels::{Color, PixelFormatEnum}, audio::AudioSpecDesired, rect::{Point, Rect}, render::{Canvas, TextureCreator, TextureQuery, WindowCanvas}, ttf::Font, video::WindowContext, Sdl};
use sdl2::messagebox::MessageBoxFlag;

#[macro_use]
extern crate bitflags;


pub(crate) trait Mem {
    fn mem_read(&self, addr: u16) -> u8;
    fn mem_write(&mut self, addr: u16, value: u8);
}

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

fn main() -> Result<(), String> {
    if let Err(msg) = run() {
        sdl2::messagebox::show_simple_message_box(MessageBoxFlag::ERROR, "YAGBE", &msg, None)
            .map_err(|err| err.to_string())?;

        return Err(msg);
    }

    Ok(())
}

fn run() -> Result<(), String> {
    let rom_path = std::env::args().nth(1).ok_or("Expected path to a ROM file")?;
    let rom = fs::read(rom_path).map_err(|_| "Could not read ROM file")?;

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let window = video_subsystem
        .window("Yet Another GameBoy Emulator", 320, 288)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    let mut cpu = cpu::Cpu::new();

    cpu.load(rom);

    let audio_subsystem = sdl_context.audio()?;
    let ttf_context = sdl2::ttf::init().map_err(|e| e.to_string())?;

    // Load a font
    let font = ttf_context.load_font("JetBrainsMono-Regular.ttf", 9)?;

    let desired_spec = AudioSpecDesired {
        freq: Some(48_000),
        channels: Some(2),
        // mono  -
        samples: None, // default sample size
    };

    let device = audio_subsystem.open_queue::<f32, _>(None, &desired_spec)?;
    device.resume();

    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    let texture_creator = canvas.texture_creator();

    let mut event_pump = sdl_context.event_pump()?;

    let mut now = Instant::now();
    let mut sleep_overhead = Duration::ZERO;

    let mut screen = texture_creator
        .create_texture_streaming(PixelFormatEnum::RGB24, 160, 144)
        .map_err(|e| e.to_string())?;

    let mut frame_delta = Duration::from_millis(16);

    let mut show_fps = false;

    'running: loop {
        let mut time_budget = Duration::from_secs_f32(1.0 / 59.73);

        let previous_now = now;

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                Event::KeyDown {
                    keycode: Some(keycode),
                    ..
                } => {
                    match keycode {
                        Keycode::F2 => show_fps = !show_fps,

                        Keycode::Down => cpu.bus.io_registers.joyp_directions &= !(1 << 3),
                        Keycode::Up => cpu.bus.io_registers.joyp_directions &= !(1 << 2),
                        Keycode::Left => cpu.bus.io_registers.joyp_directions &= !(1 << 1),
                        Keycode::Right => cpu.bus.io_registers.joyp_directions &= !(1 << 0),

                        Keycode::Return => cpu.bus.io_registers.joyp_actions &= !(1 << 3),
                        Keycode::Tab => cpu.bus.io_registers.joyp_actions &= !(1 << 2),
                        Keycode::LAlt => cpu.bus.io_registers.joyp_actions &= !(1 << 1),
                        Keycode::LCtrl => cpu.bus.io_registers.joyp_actions &= !(1 << 0),
                        _ => {}
                    }
                }
                Event::KeyUp {
                    keycode: Some(keycode),
                    ..
                } => match keycode {
                    Keycode::Down => cpu.bus.io_registers.joyp_directions |= 1 << 3,
                    Keycode::Up => cpu.bus.io_registers.joyp_directions |= 1 << 2,
                    Keycode::Left => cpu.bus.io_registers.joyp_directions |= 1 << 1,
                    Keycode::Right => cpu.bus.io_registers.joyp_directions |= 1 << 0,

                    Keycode::Return => cpu.bus.io_registers.joyp_actions |= 1 << 3,
                    Keycode::Tab => cpu.bus.io_registers.joyp_actions |= 1 << 2,
                    Keycode::LAlt => cpu.bus.io_registers.joyp_actions |= 1 << 1,
                    Keycode::LCtrl => cpu.bus.io_registers.joyp_actions |= 1 << 0,
                    _ => {}
                },
                _ => {}
            }
        }

        if cpu.run_to_frame(time_budget) {
            screen.with_lock(None, |buffer: &mut [u8], pitch: usize| {
                for (index, &color) in cpu.bus.ppu.screen.iter().enumerate() {
                    let x = index % 160;
                    let y = index / 160;

                    let color = COLORS[color as usize];

                    let offset = y * pitch + x * 3;
                    buffer[offset] = color.r;
                    buffer[offset + 1] = color.g;
                    buffer[offset + 2] = color.b;
                }
            })?;
        }

        // Draw screen
        canvas.copy(&screen, None, Some(Rect::new(0, 0, 160 * 2, 144 * 2)))?;

        if show_fps {
            render_text(&font, &mut canvas, &texture_creator, format!("{:.2}", 1.0 / frame_delta.as_secs_f32()).as_str(), Point::new(4, 4))?;
        }

        canvas.present();

        let sample_count_src = cpu.bus.apu.buffer.len();
        if sample_count_src > 0 {
            device.queue_audio(cpu.bus.apu.buffer.as_slice()).unwrap();

            cpu.bus.apu.buffer.clear();
        }

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

        now = Instant::now();

        frame_delta = now - previous_now;
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
