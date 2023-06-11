mod cpu;
mod bus;

extern crate sdl2;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use std::time::Duration;

#[macro_use]
extern crate bitflags;

use std::fs;
use sdl2::rect::Point;

pub(crate) trait Mem {
    fn mem_read(&self, addr: u16) -> u8;
    fn mem_write(&mut self, addr: u16, value: u8);
}

fn main() -> Result<(), String> {
    // TODO
    // let rom = fs::read("cpu_instrs\\individual\\02-interrupts.gb").unwrap();
    let rom = fs::read("cpu_instrs\\individual\\08-misc instrs.gb").unwrap();
    // let rom = fs::read("cpu_instrs\\individual\\11-op a,(hl).gb").unwrap();
    
    
    let mut cpu = cpu::Cpu::new();
    
    let mut log = fs::File::create("log.txt").unwrap();
    
    cpu.load(rom);

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    let window = video_subsystem
        .window("Yet Another GameBoy Emulator", 320, 288)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;

    canvas.set_scale(2.0, 2.0).unwrap();
    
    let mut event_pump = sdl_context.event_pump()?;

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
        
        canvas.set_draw_color(Color::RGB(0, 0, 255));
        canvas.clear();
        
        cpu.run_to_frame(&mut log);
        
        for (index, &color) in cpu.bus.ppu.screen.iter().enumerate() {
            canvas.set_draw_color(Color::RGB(color, color, color));
            canvas.draw_point(Point::new((index % 160) as i32, (index / 160) as i32)).unwrap();
        }

        canvas.present();
        
        ::std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60 ));
    }

    Ok(())
}
