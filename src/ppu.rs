use std::cmp::Ordering;
use bitflags::Flags;
use crate::bus::{Bus};

use crate::io_registers::{InterruptFlags, IoRegisters, LCDControl};
use crate::Mem;
use crate::pixel_fetcher::PixelFetcher;
use crate::pixel_fetcher::PixelFetcherMode::{Object};
use crate::ppu::PpuMode::{PixelTransfer, HBlank, OamLookup, VBlank};

const VRAM_BASE_ADDR: u16 = 0x8000;
const OAM_BASE_ADDR: u16 = 0xfe00;

pub struct Oam {
    pub y: u8,
    pub x: u8,
    pub oam_addr: u16,
}

pub struct Vram {
    pub vram: [u8; 0x2000],
    pub oam: [u8; 0xa0],
}

impl Vram {
    pub fn new() -> Self {
        Self {
            vram: [0; 0x2000],
            oam: [0; 0xa0],
        }
    }
}

impl Mem for Vram {
    fn mem_read(&self, addr: u16) -> u8 {
        return match addr {
            VRAM_BASE_ADDR..=0x9fff => self.vram[(addr - VRAM_BASE_ADDR) as usize],
            OAM_BASE_ADDR..=0xfe9f => self.oam[(addr - OAM_BASE_ADDR) as usize],
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

#[derive(Clone, Copy)]
#[repr(u8)]
enum PpuMode {
    HBlank = 0,
    VBlank,
    OamLookup,
    PixelTransfer,
}

impl From<u8> for PpuMode {
    fn from(value: u8) -> Self {
        match value {
            0 => HBlank,
            1 => VBlank,
            2 => OamLookup,
            3 => PixelTransfer,
            _ => panic!("Invalid value: {}", value),
        }
    }
}

pub struct Ppu {
    pub dot_counter: usize,
    pub vram: Vram,
    sprites: Vec<Oam>,
    pub screen: [u8; 160 * 144],
    screen_x: u8,
    skipped_pixels: u8,
    pixel_fetcher: PixelFetcher,
}

impl Ppu {
    pub fn new() -> Ppu {
        Ppu {
            dot_counter: 0,
            vram: Vram::new(),
            sprites: Vec::with_capacity(10),
            screen: [0; 160 * 144],
            screen_x: 0,
            skipped_pixels: 0,
            pixel_fetcher: PixelFetcher::new(),
        }
    }

    pub fn tick(&mut self, registers: &mut IoRegisters) -> bool {
        let mut result = false;

        let lcd_enable = registers.lcdc.contains(LCDControl::LCD_PPU_ENABLE);

        if !lcd_enable {
            registers.stat = registers.stat & 0b1111_1000;
        }

        if let Some(mode) = self.handle_step(registers, lcd_enable) {
            registers.stat = (registers.stat & 0b1111_1100) | (mode as u8 & 0b0000_0011);
        };

        if lcd_enable {
            self.dot_counter += 1;
        }

        if self.dot_counter == 70224 {
            self.dot_counter = 0;

            result = true;
            
            registers.ly = 0;
            registers.window_ly = 0;

            Self::set_lyc_interrupt(registers);

            registers.interrupt_flag.remove(InterruptFlags::VBLANK | InterruptFlags::LCD_STAT);
        }

        return result;
    }

    fn handle_step(&mut self, registers: &mut IoRegisters, lcd_enable: bool) -> Option<PpuMode> {
        if registers.lyc == registers.ly {
            registers.stat = registers.stat | (1 << 2);
        } else {
            registers.stat = registers.stat & !(1 << 2);
        }

        let mut mode = PpuMode::from(registers.stat & 0b0000_0011);

        let line_dot = self.dot_counter % 456;

        let bg_enable = registers.lcdc.contains(LCDControl::BG_WINDOW_ENABLE);
        let sprites_enable = registers.lcdc.contains(LCDControl::OBJ_ENABLE);
        let window_enable = registers.lcdc.contains(LCDControl::WINDOW_ENABLE) && registers.wx < 167 && registers.wy < 144;

        let is_window_scanline = window_enable && registers.ly >= registers.wy;
        let mut is_window = false;

        match mode {
            HBlank => {
                if line_dot == 0 {
                    if lcd_enable {
                        registers.ly += 1;

                        if is_window_scanline {
                            registers.window_ly += 1;
                        }

                        Self::set_lyc_interrupt(registers);
                    }

                    if registers.ly == 144 {
                        mode = VBlank;

                        if lcd_enable {
                            registers.interrupt_flag.insert(InterruptFlags::VBLANK);

                            // According to The Cycle-Accurate Game Boy Docs, OAM bit also triggers the interrupt on VBlank.
                            if registers.stat & (1 << 4) != 0 || registers.stat & (1 << 5) != 0 {
                                registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
                            }
                        }
                    } else if registers.ly < 144 {
                        mode = OamLookup;

                        self.skipped_pixels = 0;

                        self.sprites.clear();

                        if lcd_enable && registers.stat & (1 << 5) != 0 {
                            registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
                        }
                    }
                }
            }
            VBlank => {
                if self.dot_counter == 0 {
                    mode = OamLookup;

                    self.skipped_pixels = 0;

                    self.sprites.clear();

                    Self::set_lyc_interrupt(registers);
                }

                registers.ly = (self.dot_counter / 456) as u8;
            }
            OamLookup => {
                self.fetch_sprites(registers, line_dot);

                if line_dot == 80 {
                    mode = PixelTransfer;

                    self.screen_x = 0;

                    self.pixel_fetcher.clear();
                    self.fetch_bg_pixels(registers, false);

                    self.sprites.sort_by(|a, b| match a.x.cmp(&b.x) {
                        Ordering::Equal => a.oam_addr.cmp(&b.oam_addr),
                        ord => ord
                    });
                }
            }
            PixelTransfer => {
                self.pixel_fetcher.tick(&self.vram, &registers);

                if bg_enable {
                    if !is_window && window_enable && is_window_scanline && self.screen_x + 7 >= registers.wx {
                        is_window = true;

                        self.fetch_bg_pixels(registers, true);

                        return None;
                    }

                    if self.pixel_fetcher.is_empty() {
                        return None;
                    }

                    if self.skipped_pixels < registers.scx & 0x7 {
                        self.pixel_fetcher.bg_fifo.pop_front();
                        self.pixel_fetcher.obj_fifo.pop_front();

                        self.skipped_pixels += 1;

                        return None;
                    }
                }

                if sprites_enable {
                    if matches!(self.pixel_fetcher.mode, Object {..}) {
                        return None;
                    }

                    if let Some(index) = self.sprites.iter().position(|s| self.screen_x + 8 == s.x || self.screen_x == 0 && s.x < 8) {
                        let &Oam { x: sprite_x, .. } = self.sprites.get(index).unwrap();

                        if self.screen_x == 0 && sprite_x < 8 {
                            let sprite = self.sprites.remove(index);

                            let sprite_offset = 8 - sprite_x;
                            self.pixel_fetcher.fetch_obj_tile(sprite, sprite_offset);

                            return None;
                        } else if self.screen_x + 8 == sprite_x {
                            let sprite = self.sprites.remove(index);

                            self.pixel_fetcher.fetch_obj_tile(sprite, 0);

                            return None;
                        }
                    }
                }

                let bg_pixel = self.pixel_fetcher.bg_fifo.pop_front();
                let sprite_pixel = self.pixel_fetcher.obj_fifo.pop_front();

                let mut pixel = 0;
                let mut palette = registers.bgp;

                match (bg_pixel, sprite_pixel) {
                    (Some(bg_pixel), Some(sprite_pixel)) => {
                        if !bg_enable {
                            pixel = sprite_pixel.color;
                            palette = sprite_pixel.palette;
                        } else if sprites_enable {
                            if sprite_pixel.bg_over_obj && bg_pixel.color != 0 || sprite_pixel.color == 0 {
                                pixel = bg_pixel.color;
                            } else {
                                pixel = sprite_pixel.color;
                                palette = sprite_pixel.palette;
                            }
                        }
                    }
                    (Some(bg_pixel), _) => {
                        if bg_enable {
                            pixel = bg_pixel.color;
                        }
                    }
                    _ => pixel = 0,
                }

                if self.screen_x < 160 && registers.ly < 144 {
                    let color = (palette >> (pixel * 2)) & 0b0000_0011;

                    self.screen[registers.ly as usize * 160 + self.screen_x as usize] = color;

                    self.screen_x = (self.screen_x + 1) % 160;

                    if self.screen_x == 0 {
                        mode = HBlank;

                        // TODO: According to mooneye-gb, HBLANK interrupt occurs one cycle before mode switch
                        // https://github.com/wilbertpol/mooneye-gb/blob/b78dd21f0b6d00513bdeab20f7950e897a0379b3/src/hardware/gpu/mod.rs#L391
                        if lcd_enable && registers.stat & (1 << 3) != 0 {
                            registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
                        }

                        // result = true;
                    }
                }
            }
        }

        Some(mode)
    }

    fn set_lyc_interrupt(registers: &mut IoRegisters) {
        if registers.stat & (1 << 2) != 0 && registers.stat & (1 << 6) != 0 {
            registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
        }
    }

    fn fetch_sprites(&mut self, registers: &mut IoRegisters, line_dot: usize) {
        if line_dot % 2 == 0 {
            return;
        }

        if self.sprites.len() == self.sprites.capacity() {
            return;
        }

        let ly = registers.ly;

        let sprite_16 = registers.lcdc.contains(LCDControl::OBJ_SIZE);
        let sprite_height: u8 = if sprite_16 {
            16
        } else {
            8
        };

        let oam_addr = 0xfe00 + (line_dot as u16 / 2) * 4;
        let sprite_y = self.vram.mem_read(oam_addr);
        let sprite_x = self.vram.mem_read(oam_addr + 1);

        if sprite_x != 0 && ly + 16 >= sprite_y && ly + 16 < sprite_y + sprite_height {
            self.sprites.push(Oam {
                y: sprite_y,
                x: sprite_x,
                oam_addr,
            });
        }
    }

    fn fetch_bg_pixels(&mut self, registers: &IoRegisters, is_window: bool) {
        let (tile_map_row_addr, tile_offset_x, tile_row_offset) = match is_window {
            true => {
                let bit_10: u16 = if registers.lcdc.contains(LCDControl::WINDOW_TILEMAP_AREA) { 1 } else { 0 };
                let offset_y = registers.window_ly as u16 / 8;

                let tile_map_row_addr = 0b1001_1000_0000_0000 | (bit_10 << 10) | offset_y << 5;
                let tile_offset_x = (self.screen_x + 7).wrapping_sub(registers.wx) / 8;
                let tile_row_offset = registers.window_ly % 8;

                (tile_map_row_addr, tile_offset_x, tile_row_offset)
            }
            _ => {
                let bit_10: u16 = if registers.lcdc.contains(LCDControl::BG_TILEMAP_AREA) { 1 } else { 0 };
                let offset_y = registers.ly.wrapping_add(registers.scy) as u16 / 8;

                let tile_map_row_addr = 0b1001_1000_0000_0000 | (bit_10 << 10) | offset_y << 5;
                // TODO: It's possible for SCX (possibly SCY) to change mid frame under certain conditions. 
                //  This causes a full-line glitch.
                //  Need to change the fetcher to allow it to recalculate the tile offsets every 
                //  iteration to reduce it to only a few pixels in a line.
                let tile_offset_x = self.screen_x.wrapping_add(registers.scx) / 8;
                let tile_row_offset = registers.ly.wrapping_add(registers.scy) % 8;

                (tile_map_row_addr, tile_offset_x, tile_row_offset)
            }
        };

        self.pixel_fetcher.fetch_bg_tile(tile_map_row_addr, tile_offset_x, tile_row_offset);
    }
}
