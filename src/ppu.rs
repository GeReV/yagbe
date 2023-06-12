use std::collections::VecDeque;
use crate::io_registers::{InterruptFlags, IoRegisters};
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
    pub x: u8,
    pub y: u8,
    pub color: u8,
}

pub struct Ppu {
    pub dot_counter: usize,
    pub vram: [u8; 0x2000],
    pub oam: [u8; 0xa0],
    scanline_x: u8,
    fetcher_x: u8,
    fetcher_y: u8,
    bg_fifo: VecDeque<FifoEntry>,
    sprite_fifo: VecDeque<FifoEntry>,
    pub screen: [u8; 160 * 144],
    tiles: [u8; 20 * 18],
}

impl Ppu {
    pub fn new() -> Ppu {
        Ppu {
            dot_counter: 0,
            vram: [0; 0x2000],
            oam: [0; 0xa0],
            scanline_x: 0,
            fetcher_x: 0,
            fetcher_y: 0,
            bg_fifo: VecDeque::with_capacity(16),
            sprite_fifo: VecDeque::with_capacity(16),
            screen: [0; 160 * 144],
            tiles: [255; 20 * 18],
        }
    }

    pub fn tick(&mut self, registers: &mut IoRegisters) -> bool {
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

                let pixel_offset_y = registers.ly.wrapping_add(registers.scy);
                let tile_offset_y = pixel_offset_y / 8;
                let tile_offset_x = ((registers.scx / 8) + self.fetcher_x) & 0x1f;

                self.fetcher_x = (self.fetcher_x + 1) & 0x1f;
                
                let tile_index_addr = bg_tile_map_addr + (tile_offset_y as u16) * 32 + tile_offset_x as u16;

                let tile_index = self.mem_read(tile_index_addr);
                
                if tile_offset_y < 18 && self.fetcher_x < 20 {
                    self.tiles[tile_offset_y as usize * 20 + self.fetcher_x as usize] = tile_index;
                }

                let tile_data_area = (registers.lcdc >> 4) & 1;
                let tile_vram_addr: u16 = match (tile_data_area, tile_index) {
                    (1, _) => 0x8000 + (tile_index as u16 * 16),
                    (0, 0..=127) => 0x9000 + (tile_index as u16 * 16),
                    (0, 128..=255) => 0x8800 + (tile_index as u16 * 16),
                    _ => unreachable!()
                };

                let tile_vram_y_offset = pixel_offset_y as u16 % 8;
                let tile_byte_lo = self.mem_read(tile_vram_addr + tile_vram_y_offset * 2 + 0);
                let tile_byte_hi = self.mem_read(tile_vram_addr + tile_vram_y_offset * 2 + 1);

                for i in 0..=7 {
                    let color = (((tile_byte_hi >> (7 - i)) & 0b0000_0001) << 1) | (tile_byte_lo >> (7 - i) & 0b0000_0001);

                    let pixel = FifoEntry {
                        x: tile_offset_x * 8 + i,
                        y: registers.ly,
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
