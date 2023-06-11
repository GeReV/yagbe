use std::collections::VecDeque;
use bitflags::Flags;
use crate::Mem;

/// Memory Map
/// 0000	3FFF	16 KiB ROM bank 00	From cartridge, usually a fixed bank
/// 4000	7FFF	16 KiB ROM Bank 01~NN	From cartridge, switchable bank via mapper (if any)
/// 8000	9FFF	8 KiB Video RAM (VRAM)	In CGB mode, switchable bank 0/1
/// A000	BFFF	8 KiB External RAM	From cartridge, switchable bank if any
/// C000	CFFF	4 KiB Work RAM (WRAM)	
/// D000	DFFF	4 KiB Work RAM (WRAM)	In CGB mode, switchable bank 1~7
/// E000	FDFF	Mirror of C000~DDFF (ECHO RAM)	Nintendo says use of this area is prohibited.
/// FE00	FE9F	Sprite attribute table (OAM)	
/// FEA0	FEFF	Not Usable	Nintendo says use of this area is prohibited
/// FF00	FF7F	I/O Registers	
/// FF80	FFFE	High RAM (HRAM)	
/// FFFF	FFFF	Interrupt Enable register (IE)
///
/// I/O Ranges    
/// Start	End	    First appeared	Purpose
/// $FF00		    DMG	            Joypad input
/// $FF01	$FF02	DMG	            Serial transfer
/// $FF04	$FF07	DMG	            Timer and divider
/// $FF10	$FF26	DMG	            Audio
/// $FF30	$FF3F	DMG	            Wave pattern
/// $FF40	$FF4B	DMG	            LCD Control, Status, Position, Scrolling, and Palettes
/// $FF4F		    CGB         	VRAM Bank Select
/// $FF50		    DMG	            Set to non-zero to disable boot ROM
/// $FF51	$FF55	CGB	            VRAM DMA
/// $FF68	$FF69	CGB	            BG / OBJ Palettes
/// $FF70		    CGB	            WRAM Bank Select

struct FifoEntry {
    pub  x: u8,
    pub  y: u8,
    pub  color: u8,
}

pub struct Ppu {
    pub dot_counter: usize,
    pub vram: [u8; 0x2000],
    pub oam: [u8; 0x9f],
    scanline_x: u8,
    fetcher_x: u8,
    fetcher_y: u8,
    bg_fifo: VecDeque<FifoEntry>,
    sprite_fifo: VecDeque<FifoEntry>,
    pub screen: [u8; 160 * 144],
}

impl Ppu {
    pub fn new() -> Ppu {
        Ppu {
            dot_counter: 0,
            vram: [0; 0x2000],
            oam: [0; 0x9f],
            scanline_x: 0,
            fetcher_x: 0,
            fetcher_y: 0,
            bg_fifo: VecDeque::with_capacity(16),
            sprite_fifo: VecDeque::with_capacity(16),
            screen: [0; 160 * 144],
        }
    }

    pub fn tick(&mut self, registers: &mut IoRegisters, ) -> bool {
        registers.ly = (self.dot_counter / 456) as u8;

        if registers.lyc == registers.ly {
            registers.stat = registers.stat & 0b1111_1011 | 0b0000_0100;
            
            registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
        } else {
            registers.stat = registers.stat & 0b1111_1011;
        } 

        let mut mode = registers.stat & 0b0000_0011;

        if registers.ly == 144 {
            mode = 1;

            registers.stat = registers.stat & 0b1110_1111 | 0b0001_0000;

            registers.interrupt_flag.insert(InterruptFlags::VBLANK | InterruptFlags::LCD_STAT);
        } else {
            // TODO: This should be used to take into account at which step we are.
            let line_dot = self.dot_counter % 456;
            
            if line_dot == 80 {
                mode = 3;
                
                self.bg_fifo.clear();
                self.sprite_fifo.clear();
            }
            
            if mode == 3 {
                let window_tile_map_addr: usize = if registers.lcdc & (1 << 6) == 0 {
                    0x9800
                } else {
                    0x9c00
                };

                let bg_tile_map_addr: u16 = if registers.lcdc & (1 << 3) == 0 {
                    0x9800
                } else {
                    0x9c00
                };

                let tile_offset_y = registers.ly.wrapping_add(registers.scy);
                let tile_offset_x = ((registers.scx / 8) + self.fetcher_x) & 0x1f;
                
                self.fetcher_x = (self.fetcher_x + 1) & 0x1f;

                let tile_index = self.mem_read(bg_tile_map_addr + (tile_offset_y / 8).wrapping_mul(32) as u16 + tile_offset_x as u16);
                
                let tile_data_area = (registers.lcdc >> 4) & 1;
                let tile_vram_addr: u16 = match (tile_data_area, tile_index) {
                    (1, _) => 0x8000 + (tile_index as u16 * 16) ,
                    (0, 0..=127) => 0x9000 + (tile_index as u16 * 16),
                    (0, 128..=255) => 0x8800 + (tile_index  as u16 * 16),
                    _ => unreachable!()
                };
                
                let tile_vram_y_offset = tile_offset_y as u16 % 8;
                let tile_byte_lo = self.mem_read(tile_vram_addr + tile_vram_y_offset * 2 + 0);
                let tile_byte_hi = self.mem_read(tile_vram_addr + tile_vram_y_offset * 2 + 1);

                for i in 0..=7 {
                    let color = (((tile_byte_hi >> (7 - i)) & 0b0000_0001) << 1) | (tile_byte_lo >> (7 - i) & 0b0000_0001);

                    let pixel = FifoEntry {
                        x: tile_offset_x * 8 + i,
                        y: tile_offset_y,
                        color,
                    };
                    self.bg_fifo.push_back(pixel);
                }

                while !self.bg_fifo.is_empty() {
                    let pixel = self.bg_fifo.pop_front().unwrap();
                
                    if pixel.x < 160 && pixel.y < 144 {
                        let color = registers.bgp >> (pixel.color * 2) & 0b0000_0011;
                
                        self.screen[pixel.y as usize * 160 + pixel.x as usize] = 255 - color * 64;
                    }
                    
                    self.scanline_x = self.scanline_x.wrapping_add(1);
                    
                    if self.scanline_x == 0 {
                        mode = 0;

                        registers.stat = registers.stat & 0b1111_0111 | 0b0000_1000;

                        registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
                    }
                }
            }
        }

        self.dot_counter += 1;
        
        let mut result = false;
        
        if self.dot_counter == 70224 {
            self.dot_counter = 0;
            mode = 2;

            registers.interrupt_flag.remove(InterruptFlags::VBLANK | InterruptFlags::LCD_STAT);

            registers.stat = registers.stat & 0b1101_1111 | 0b0010_0000; 
            
            result = true;
        }

        registers.stat = (registers.stat & 0b1111_1100) | (mode & 0b0000_0011);
        
        return result;
    }
}

impl Mem for Ppu {
    fn mem_read(&self, addr: u16) -> u8 {
        return match addr {
            0x8000..=0x9fff => self.vram[(addr - 0x8000) as usize],
            0xfe00..=0xfe9f => self.oam[(addr - 0xfe00) as usize],
            _ => unreachable!()
        };
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0x9fff => {
                self.vram[(addr - 0x8000) as usize] = value;
            }
            0xfe00..=0xfe9f => self.oam[(addr - 0xfe00) as usize] = value,
            _ => unreachable!()
        }
    }
}

bitflags! {
    #[derive(Default, Copy, Clone, PartialEq, Eq)]
    pub struct InterruptFlags : u8 {
        const VBLANK = 1 << 0;
        const LCD_STAT = 1 << 1;
        const TIMER = 1 << 2;
        const SERIAL = 1 << 3;
        const JOYPAD = 1 << 4;
    }
}

#[derive(Default)]
pub struct IoRegisters {
    pub joyp: u8,
    pub sb: u8,
    pub sc: u8,
    pub div: u8,
    pub tima: u8,
    pub tma: u8,
    pub tac: u8,
    pub interrupt_flag: InterruptFlags,
    pub nr10: u8,
    pub nr11: u8,
    pub nr12: u8,
    pub nr13: u8,
    pub nr14: u8,
    pub nr21: u8,
    pub nr22: u8,
    pub nr23: u8,
    pub nr24: u8,
    pub nr30: u8,
    pub nr31: u8,
    pub nr32: u8,
    pub nr33: u8,
    pub nr34: u8,
    pub nr41: u8,
    pub nr42: u8,
    pub nr43: u8,
    pub nr44: u8,
    pub nr50: u8,
    pub nr51: u8,
    pub nr52: u8,
    pub wave_ram: [u8; 0x10],
    pub lcdc: u8,
    pub stat: u8,
    pub scy: u8,
    pub scx: u8,
    pub ly: u8,
    pub lyc: u8,
    pub dma: u8,
    pub bgp: u8,
    pub obp0: u8,
    pub obp1: u8,
    pub wy: u8,
    pub wx: u8,
    pub interrupt_enable: InterruptFlags,
}

impl Mem for IoRegisters {
    fn mem_read(&self, addr: u16) -> u8 {
        return match addr {
            0xff00 => self.joyp,
            0xff01 => self.sb,
            0xff02 => self.sc,
            0xff04 => self.div,
            0xff05 => self.tima,
            0xff06 => self.tma,
            0xff07 => self.tac,
            0xff0f => self.interrupt_flag.bits(),
            0xff10 => self.nr10,
            0xff11 => self.nr11,
            0xff12 => self.nr12,
            0xff13 => panic!("cannot read nr13 register"),
            0xff14 => self.nr14,
            0xff16 => self.nr21,
            0xff17 => self.nr22,
            0xff18 => panic!("cannot read nr23 register"),
            0xff19 => self.nr24,
            0xff1a => self.nr30,
            0xff1b => panic!("cannot read nr31 register"),
            0xff1c => self.nr32,
            0xff1d => panic!("cannot read nr33 register"),
            0xff1e => self.nr34,
            0xff20 => panic!("cannot read nr41 register"),
            0xff21 => self.nr42,
            0xff22 => self.nr43,
            0xff23 => self.nr44,
            0xff24 => self.nr50,
            0xff25 => self.nr51,
            0xff26 => self.nr52,
            0xff30..=0xff3f => self.wave_ram[(addr - 0xff30) as usize],
            0xff40 => self.lcdc,
            0xff41 => self.stat,
            0xff42 => self.scy,
            0xff43 => self.scx,
            0xff44 => self.ly,
            0xff45 => self.lyc,
            0xff46 => self.dma,
            0xff47 => self.bgp,
            0xff48 => self.obp0,
            0xff49 => self.obp1,
            0xff4a => self.wy,
            0xff4b => self.wx,
            0xff4d => panic!("cgb only"),
            0xff4f => panic!("cgb only"),
            0xff51..=0xff56 => panic!("cgb only"),
            0xff68..=0xff6c => panic!("cgb only"),
            0xff70 => panic!("cgb only"),
            0xff76 => panic!("cgb only"),
            0xff77 => panic!("cgb only"),
            0xffff => self.interrupt_enable.bits(),
            _ => panic!("invalid IO register address")
        };
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        return match addr {
            0xff00 => self.joyp = value, // TODO: Mixed?
            0xff01 => self.sb = value,
            0xff02 => self.sc = value,
            0xff04 => self.div = value,
            0xff05 => self.tima = value,
            0xff06 => self.tma = value,
            0xff07 => self.tac = value,
            0xff0f => self.interrupt_flag = InterruptFlags::from_bits_truncate(value),
            0xff10 => self.nr10 = value,
            0xff11 => self.nr11 = value, // TODO: Mixed?
            0xff12 => self.nr12 = value,
            0xff13 => self.nr13 = value,
            0xff14 => self.nr14 = value, // TODO: Mixed?
            0xff16 => self.nr21 = value, // TODO: Mixed?
            0xff17 => self.nr22 = value,
            0xff18 => self.nr23 = value,
            0xff19 => self.nr24 = value, // TODO: Mixed?
            0xff1a => self.nr30 = value,
            0xff1b => self.nr31 = value,
            0xff1c => self.nr32 = value,
            0xff1d => self.nr33 = value,
            0xff1e => self.nr34 = value, // TODO: Mixed?
            0xff20 => self.nr41 = value,
            0xff21 => self.nr42 = value,
            0xff22 => self.nr43 = value,
            0xff23 => self.nr44 = value, // TODO: Mixed?
            0xff24 => self.nr50 = value,
            0xff25 => self.nr51 = value,
            0xff26 => self.nr52 = value, // TODO: Mixed?
            0xff30..=0xff3f => self.wave_ram[(addr - 0xff30) as usize] = value,
            0xff40 => self.lcdc = value,
            0xff41 => self.stat = value, // TODO: Mixed?
            0xff42 => self.scy = value,
            0xff43 => self.scx = value,
            0xff44 => panic!("cannot write ly register"),
            0xff45 => self.lyc = value,
            0xff46 => self.dma = value,
            0xff47 => self.bgp = value,
            0xff48 => self.obp0 = value,
            0xff49 => self.obp1 = value,
            0xff4a => self.wy = value,
            0xff4b => self.wx = value,
            0xff4d => panic!("cgb only"),
            0xff4f => panic!("cgb only"),
            0xff51..=0xff56 => panic!("cgb only"),
            0xff68..=0xff6c => panic!("cgb only"),
            0xff70 => panic!("cgb only"),
            0xff76 => panic!("cgb only"),
            0xff77 => panic!("cgb only"),
            0xffff => self.interrupt_enable = InterruptFlags::from_bits_truncate(value),
            _ => panic!("invalid IO register address")
        };
    }
}


pub struct Bus {
    pub bank0: [u8; 0x4000],
    pub bank1: [u8; 0x4000],
    pub wram: [u8; 0x2000],
    pub hram: [u8; 0x7f],
    pub ppu: Ppu,
    pub io_registers: IoRegisters,
}

impl Bus {
    pub fn new() -> Self {
        Bus {
            bank0: [0; 0x4000],
            bank1: [0; 0x4000],
            wram: [0; 0x2000],
            hram: [0; 0x7f],
            ppu: Ppu::new(),
            io_registers: Default::default(),
        }
    }
}

impl Mem for Bus {
    fn mem_read(&self, addr: u16) -> u8 {
        return match addr {
            0x0000..=0x3fff => self.bank0[addr as usize],
            0x4000..=0x7fff => self.bank1[(addr - 0x4000) as usize],
            0x8000..=0x9fff => self.ppu.mem_read(addr),
            0xa000..=0xbfff => todo!("external ram"),
            0xc000..=0xdfff => self.wram[(addr - 0xc000) as usize],
            0xe000..=0xfdff => self.wram[(addr - 0xe000) as usize],
            0xfe00..=0xfe9f => 0,
            0xfea0..=0xfeff => {
                // TODO: If OAM blocked
                // TODO: OAM corruption, return 0?
                return 0xff;
            }
            0xff00..=0xff7f => self.io_registers.mem_read(addr),
            0xff80..=0xfffe => self.hram[(addr - 0xff80) as usize],
            0xffff => self.io_registers.mem_read(addr),
            _ => unreachable!()
        };
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1fff => {
                let enable_ram = value;
            }
            0x2000..=0x3fff => {
                // TODO: Make mask based on number of banks.
                let mut bank = value & 0b0001_1111;

                let allow_bank0_mirroring = false; // TODO: If ROM size is < 256KiB

                if allow_bank0_mirroring && bank == 0x10 {
                    bank = 0x00;
                } else if bank == 0x00 {
                    bank = 0x01;
                }
            }
            0x4000..=0x5fff => {
                let value = value & 0b0000_0011;
            }
            0x6000..=0x7fff => {
                // 00 = Simple Banking Mode (default)
                //      0000–3FFF and A000–BFFF locked to bank 0 of ROM/RAM
                // 01 = RAM Banking Mode / Advanced ROM Banking Mode
                //      0000–3FFF and A000–BFFF can be bank-switched via the 4000–5FFF bank register
                let value = value & 0b0000_0001;
            }
            0x8000..=0x9fff => self.ppu.mem_write(addr, value),
            0xc000..=0xdfff => {
                self.wram[(addr - 0xc000) as usize] = value;
            },
            0xe000..=0xfdff => self.wram[(addr - 0xe000) as usize] = value,
            0xfe00..=0xfe9f => self.ppu.mem_write(addr, value),
            0xfea0..=0xfeff => panic!("not usable"),
            0xff00..=0xff7f => self.io_registers.mem_write(addr, value),
            0xff80..=0xfffe => self.hram[(addr - 0xff80) as usize] = value,
            0xffff => self.io_registers.mem_write(addr, value),
            _ => unreachable!()
        }
    }
}