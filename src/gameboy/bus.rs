use super::{
    apu::Apu,
    io_registers::IoRegisters,
    Mem,
    ppu::Ppu,
    cartridge::Cartridge,
};

pub struct Bus {
    pub ppu: Ppu,
    pub apu: Apu,
    pub io_registers: IoRegisters,
    cartridge: Option<Cartridge>,
    wram: [u8; 0x2000],
    hram: [u8; 0x7f],
}

impl Bus {
    pub fn new() -> Self {
        Bus {
            ppu: Ppu::new(),
            apu: Apu::new(),
            io_registers: IoRegisters::new(),
            cartridge: None,
            wram: [0; 0x2000],
            hram: [0; 0x7f],
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn load(&mut self, program: Vec<u8>) {
        self.reset();
        self.cartridge = Some(Cartridge::load(program));
    }
}

impl Mem for Bus {
    fn mem_read(&self, addr: u16) -> u8 {
        // TODO: On DMG, during OAM DMA, the CPU can access only HRAM (memory at $FF80-$FFFE).
        // if self.io_registers.dma_counter > 0 && !(0xff80..=0xfffe).contains(&addr) {
        //     return 0xff;
        // }

        return match addr {
            0x0000..=0x7fff | 0xa000..=0xbfff => match &self.cartridge {
                Some(cartridge) => cartridge.mem_read(addr),
                _ => 0x00
            },
            0x8000..=0x9fff => self.ppu.vram.mem_read(addr),
            0xc000..=0xdfff => self.wram[(addr - 0xc000) as usize],
            0xe000..=0xfdff => self.wram[(addr - 0xe000) as usize],
            0xfe00..=0xfe9f => 0,
            0xfea0..=0xfeff => {
                // TODO: If OAM blocked
                // TODO: OAM corruption, return 0?
                return 0xff;
            }
            0xff10..=0xff3f => self.apu.mem_read(addr),
            0xff00..=0xff0f | 0xff40..=0xff7f => self.io_registers.mem_read(addr),
            0xff80..=0xfffe => self.hram[(addr - 0xff80) as usize],
            0xffff => self.io_registers.mem_read(addr),
            _ => unreachable!()
        };
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7fff | 0xa000..=0xbfff => match self.cartridge {
                Some(ref mut cartridge) => cartridge.mem_write(addr, value),
                _ => {}
            }
            0x8000..=0x9fff => self.ppu.vram.mem_write(addr, value),
            0xc000..=0xdfff => self.wram[(addr - 0xc000) as usize] = value,
            0xe000..=0xfdff => self.wram[(addr - 0xe000) as usize] = value,
            0xfe00..=0xfe9f => self.ppu.vram.mem_write(addr, value),
            0xfea0..=0xfeff => {} // panic!("not usable"),
            0xff10..=0xff3f => self.apu.mem_write(addr, value),
            0xff00..=0xff0f | 0xff40..=0xff7f => self.io_registers.mem_write(addr, value),
            0xff80..=0xfffe => self.hram[(addr - 0xff80) as usize] = value,
            0xffff => self.io_registers.mem_write(addr, value),
            _ => unreachable!()
        }
    }
}