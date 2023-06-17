mod cpu;
mod bus;
mod ppu;
mod io_registers;
mod cpu_registers;

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
    // let rom = fs::read("test\\cpu_instrs\\cpu_instrs.gb").unwrap();                          // Pass
    // let rom = fs::read("test\\instr_timing\\instr_timing.gb").unwrap();                      // Pass
    // let rom = fs::read("test\\interrupt_time\\interrupt_time.gb").unwrap();                  // Requires CGB
    // let rom = fs::read("test\\mem_timing\\mem_timing.gb").unwrap();
    // let rom = fs::read("test\\mem_timing\\individual\\01-read_timing.gb").unwrap();          
    // let rom = fs::read("test\\mem_timing\\individual\\02-write_timing.gb").unwrap();
    // let rom = fs::read("test\\mem_timing\\individual\\03-modify_timing.gb").unwrap();
    // let rom = fs::read("test\\mem_timing-2\\mem_timing.gb").unwrap();
    // let rom = fs::read("test\\mem_timing-2\\rom_singles\\01-read_timing.gb").unwrap();
    // let rom = fs::read("test\\mem_timing-2\\rom_singles\\02-write_timing.gb").unwrap();
    // let rom = fs::read("test\\mem_timing-2\\rom_singles\\03-modify_timing.gb").unwrap();
    let rom = fs::read("test\\dmg-acid2\\dmg-acid2.gb").unwrap();
    // let rom = fs::read("test\\mario.gb").unwrap();
    // let rom = fs::read("test\\tetris.gb").unwrap();

    let file = fs::File::create("log.txt").unwrap();
    let writer = std::io::LineWriter::with_capacity(512 * 1024 * 1024, file);
    
    let mut cpu = cpu::Cpu::new(writer);
    
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
        
        cpu.run_to_frame();
        
        for (index, &color) in cpu.bus.ppu.screen.iter().enumerate() {
            canvas.set_draw_color(Color::RGB(color, color, color));
            canvas.draw_point(Point::new((index % 160) as i32, (index / 160) as i32)).unwrap();
        }

        canvas.present();
        
        ::std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60 ));
    }
    
    Ok(())
}
