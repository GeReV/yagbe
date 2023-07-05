use std::time::Duration;
use crate::gameboy::cpu::Cpu;
use self::bus::Bus;

mod cpu;
mod bus;
mod ppu;
mod io_registers;
mod cpu_registers;
mod apu;
mod pixel_fetcher;

const MCYCLE_DURATION: Duration = Duration::from_nanos((1e9 / 1.048576e6) as u64);

pub(crate) trait Mem {
    fn mem_read(&self, addr: u16) -> u8;
    fn mem_write(&mut self, addr: u16, value: u8);
}

pub enum Buttons {
    Right,
    Left,
    Up,
    Down,

    B,
    A,
    Select,
    Start,
}

pub struct GameBoy {
    bus: Bus,
    cpu: Cpu,
    loaded: bool,
    accumulator: Duration,
}

impl GameBoy {
    pub fn new() -> Self {
        Self {
            bus: Bus::new(),
            cpu: Cpu::new(),
            loaded: false,
            accumulator: Duration::ZERO,
        }
    }

    pub fn load(&mut self, program: Vec<u8>) {
        self.bus.load(program);

        self.loaded = true;
    }

    pub fn run_to_frame(&mut self, time_budget: Duration) -> bool {
        if !self.loaded {
            return false;
        }

        self.accumulator += time_budget;

        loop {
            let m_cycles = self.cpu.tick(&mut self.bus);
            let t_cycles = m_cycles.t_cycles();

            self.accumulator = self.accumulator.saturating_sub(MCYCLE_DURATION * m_cycles.into());

            for _ in 0..t_cycles {
                if self.bus.ppu.tick(&mut self.bus.io_registers) {
                    return true;
                }
            }

            for _ in 0..m_cycles.into() {
                self.bus.apu.tick(&self.bus.io_registers);
            }

            if self.accumulator.is_zero() {
                return false;
            }
        }
    }

    pub fn screen(&self) -> &[u8; 160 * 144] {
        return &self.bus.ppu.screen;
    }

    pub fn extract_audio_buffer(&mut self) -> Vec<f32> {
        return self.bus.apu.extract_audio_buffer();
    }

    pub fn button_pressed(&mut self, button: Buttons) {
        match button {
            Buttons::Right => self.bus.io_registers.joyp_directions &= !(1 << 0),
            Buttons::Left => self.bus.io_registers.joyp_directions &= !(1 << 1),
            Buttons::Up => self.bus.io_registers.joyp_directions &= !(1 << 2),
            Buttons::Down => self.bus.io_registers.joyp_directions &= !(1 << 3),
            Buttons::B => self.bus.io_registers.joyp_actions &= !(1 << 0),
            Buttons::A => self.bus.io_registers.joyp_actions &= !(1 << 1),
            Buttons::Select => self.bus.io_registers.joyp_actions &= !(1 << 2),
            Buttons::Start => self.bus.io_registers.joyp_actions &= !(1 << 3),
        };
    }

    pub fn button_released(&mut self, button: Buttons) {
        match button {
            Buttons::Right => self.bus.io_registers.joyp_directions |= 1 << 0,
            Buttons::Left => self.bus.io_registers.joyp_directions |= 1 << 1,
            Buttons::Up => self.bus.io_registers.joyp_directions |= 1 << 2,
            Buttons::Down => self.bus.io_registers.joyp_directions |= 1 << 3,
            Buttons::B => self.bus.io_registers.joyp_actions |= 1 << 0,
            Buttons::A => self.bus.io_registers.joyp_actions |= 1 << 1,
            Buttons::Select => self.bus.io_registers.joyp_actions |= 1 << 2,
            Buttons::Start => self.bus.io_registers.joyp_actions |= 1 << 3,
        };
    }
}