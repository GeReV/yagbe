use std::cmp::Ordering;
use std::collections::VecDeque;
use std::ops::IndexMut;
use bitflags::Flags;

use crate::io_registers::{InterruptFlags, IoRegisters, LCDControl};
use crate::Mem;
use crate::ppu::PixelFetcherMode::{Background, Object, Window};
use crate::ppu::PixelFetcherState::{GetTileId, GetTileRowHigh, GetTileRowLow, PushPixels, Sleep};

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

fn fetch_tile_bytes(vram: &Vram, registers: &IoRegisters, tile_index: u8, pixel_offset_y: u8) -> (u8, u8) {
    let tile_data_area = registers.lcdc.contains(LCDControl::BG_TILEDATA_AREA);
    let tile_vram_addr: u16 = match (tile_data_area, tile_index) {
        (true, _) => 0x8000 + (tile_index as u16 * 16),
        (false, 0..=127) => 0x9000 + (tile_index as u16 * 16),
        (false, 128..=255) => 0x8800 + ((tile_index - 128) as u16 * 16),
        _ => unreachable!()
    };

    let tile_vram_y_offset = pixel_offset_y as u16 % 8;
    let tile_byte_lo = vram.mem_read(tile_vram_addr + tile_vram_y_offset * 2 + 0);
    let tile_byte_hi = vram.mem_read(tile_vram_addr + tile_vram_y_offset * 2 + 1);

    (tile_byte_lo, tile_byte_hi)
}

enum PixelFetcherState {
    Sleep,
    GetTileId,
    GetTileRowLow {
        tile_index: u8,
    },
    GetTileRowHigh {
        tile_address: u16,
        tile_byte_lo: u8,
    },
    PushPixels {
        tile_byte_lo: u8,
        tile_byte_hi: u8,
    },
}

enum PixelFetcherMode {
    Background {
        tile_address: u16,
    },
    Window {
        tile_address: u16,
    },
    Object {
        x: u8,
        y: u8,
        attributes: u8,
        tile_index: u8,
    },
}

struct PixelFetcher {
    dot_counter: usize,
    pub state: PixelFetcherState,
    mode: PixelFetcherMode,
    bg_fifo: VecDeque<u8>,
    obj_fifo: VecDeque<SpritePixel>,
}

impl PixelFetcher {
    pub fn new() -> Self {
        Self {
            dot_counter: 0,
            state: Sleep,
            mode: Background { tile_address: 0x9800 },
            bg_fifo: VecDeque::with_capacity(8),
            obj_fifo: VecDeque::with_capacity(8),
        }
    }

    pub fn tick(&mut self, vram: &Vram, registers: &IoRegisters) {
        self.dot_counter = self.dot_counter.wrapping_add(1);

        match self.state {
            Sleep => {}
            GetTileId => {
                if self.dot_counter % 2 == 1 {
                    return;
                }

                let tile_index = match self.mode {
                    Background { tile_address } => vram.mem_read(tile_address),
                    Window { tile_address } => vram.mem_read(tile_address),
                    Object { tile_index, .. } => tile_index
                };

                self.state = GetTileRowLow {
                    tile_index,
                };
            }
            GetTileRowLow { tile_index } => {
                if self.dot_counter % 2 == 1 {
                    return;
                }

                // https://github.com/gbdev/pandocs/blob/bbdc0ef79ba46dcc8183ad788b651ae25b52091d/src/Rendering_Internals.md#get-tile-row-low
                // For BG/Window tiles, bit 12 depends on LCDC bit 4. If that bit is set ("$8000 mode"), then bit 12 is always 0; otherwise ("$8800 mode"), it is the negation of the tile ID's bit 7. 
                // The full logical formula is thus: !((LCDC & $10) || (tileID & $80)) (see gate VUZA in the schematics).
                let bit_12 = !(registers.lcdc.contains(LCDControl::BG_TILEDATA_AREA) || (tile_index & (1 << 7) != 0));
                let bit_12: u16 = if bit_12 { 1 } else { 0 };

                let tile_index = tile_index as u16;

                let tile_address = match self.mode {
                    Background { .. } => {
                        0b1000_0000_0000_0000 | (bit_12 << 12) | tile_index << 4 | ((registers.ly.wrapping_add(registers.scy) % 8) << 1) as u16
                    }
                    Window { .. } => {
                        0b1000_0000_0000_0000 | (bit_12 << 12) | tile_index << 4 | ((registers.window_ly % 8) as u16) << 1
                    }
                    Object { y, attributes, .. } => {
                        let mut row_offset = registers.ly.wrapping_sub(y % 8) % 8;

                        let flip_sprite_v = attributes & (1 << 6) != 0;

                        if flip_sprite_v {
                            row_offset = 7 - row_offset;
                        }

                        0b1000_0000_0000_0000 | tile_index << 4 | (row_offset << 1) as u16
                    }
                };

                let tile_byte_lo = vram.mem_read(tile_address);

                self.state = GetTileRowHigh {
                    tile_byte_lo,
                    tile_address,
                };
            }
            GetTileRowHigh { tile_byte_lo, tile_address } => {
                if self.dot_counter % 2 == 1 {
                    return;
                }

                let tile_byte_hi = vram.mem_read(tile_address + 1);

                self.state = PushPixels {
                    tile_byte_lo,
                    tile_byte_hi,
                };
            }
            PushPixels { tile_byte_lo, tile_byte_hi } => {
                if let Object { x, attributes, .. } = self.mode {
                    while self.obj_fifo.len() < 8 {
                        self.obj_fifo.push_back(SpritePixel {
                            x: x as isize - 8 + self.obj_fifo.len() as isize,
                            color: 0,
                            bg_over_obj: false,
                            palette: registers.obp0,
                        });
                    }

                    let mut pixels = VecDeque::<SpritePixel>::with_capacity(8);

                    let mut insert_pixel = |pixel: u8, i: usize| {
                        pixels.push_back(SpritePixel {
                            x: x as isize - 8 + i as isize,
                            color: pixel,
                            bg_over_obj: attributes & (1 << 7) != 0,
                            palette: if attributes & (1 << 4) == 0 {
                                registers.obp0
                            } else {
                                registers.obp1
                            },
                        });
                    };

                    let flip_sprite_h = attributes & (1 << 5) != 0;
                    if flip_sprite_h {
                        for i in 0..=7 {
                            let pixel = (((tile_byte_hi >> i) & 1) << 1) | (tile_byte_lo >> i & 1);

                            insert_pixel(pixel, i);
                        }
                    } else {
                        for i in 0..=7 {
                            let pixel = (((tile_byte_hi >> (7 - i)) & 1) << 1) | (tile_byte_lo >> (7 - i) & 1);

                            insert_pixel(pixel, i);
                        }
                    }

                    let start_index = self.obj_fifo.iter().position(|s| s.x == pixels[0].x);

                    if let Some(mut index) = start_index {
                        while index < 8 {
                            match self.obj_fifo.get(index) {
                                Some(s) if s.color == 0 => {
                                    let item = self.obj_fifo.index_mut(index);
                                    *item = pixels.pop_front().unwrap();
                                }
                                None => {
                                    let item = self.obj_fifo.index_mut(index);
                                    *item = pixels.pop_front().unwrap();
                                }
                                _ => {}
                            }

                            index += 1;
                        }
                    }

                    self.state = Sleep;
                } else if self.bg_fifo.is_empty() {
                    for i in 0..=7 {
                        let pixel = (((tile_byte_hi >> (7 - i)) & 1) << 1) | (tile_byte_lo >> (7 - i) & 1);

                        self.bg_fifo.push_back(pixel);
                    }

                    self.state = Sleep;
                }
            }
        }
    }

    pub fn fetch_bg_tile(&mut self, tile_map_addr: u16) {
        self.bg_fifo.clear();

        self.dot_counter = 0;
        self.state = GetTileId;
        self.mode = Background { tile_address: tile_map_addr };
    }

    pub fn fetch_window_tile(&mut self, tile_map_addr: u16) {
        self.bg_fifo.clear();

        self.dot_counter = 0;
        self.state = GetTileId;
        self.mode = Window { tile_address: tile_map_addr };
    }

    pub fn fetch_obj_tile(&mut self, oam: &Oam) {
        self.dot_counter = 0;
        self.state = GetTileId;
        self.mode = Object {
            x: oam.x,
            y: oam.y,
            attributes: oam.attributes,
            tile_index: oam.tile_index,
        };
    }
}

struct SpritePixel {
    pub x: isize,
    pub color: u8,
    pub palette: u8,
    pub bg_over_obj: bool,
}

struct Oam {
    pub y: u8,
    pub x: u8,
    pub tile_index: u8,
    pub attributes: u8,
    oam_index: u8,
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

pub struct Ppu {
    pub dot_counter: usize,
    pub vram: Vram,
    scanline_x: u8,
    sprites: Vec<Oam>,
    pub screen: [u8; 160 * 144],
    pixel_fetcher: PixelFetcher,
    delay: usize,
}

impl Ppu {
    pub fn new() -> Ppu {
        Ppu {
            dot_counter: 0,
            vram: Vram::new(),
            scanline_x: 0,
            sprites: Vec::with_capacity(10),
            screen: [0; 160 * 144],
            pixel_fetcher: PixelFetcher::new(),
            delay: 0,
        }
    }

    pub fn tick(&mut self, registers: &mut IoRegisters) -> bool {
        let mut result = false;

        let lcd_enable = registers.lcdc.contains(LCDControl::LCD_PPU_ENABLE);

        if !lcd_enable {
            registers.stat = registers.stat & 0b1111_1000;
        }

        if registers.lyc == registers.ly {
            if lcd_enable && registers.stat & (1 << 2) == 0 && registers.stat & (1 << 6) != 0 {
                registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
            }

            registers.stat = registers.stat | (1 << 2);
        } else {
            registers.stat = registers.stat & 0b1111_1011;
        }

        let mut mode = registers.stat & 0b0000_0011;

        let line_dot = self.dot_counter % 456;

        let bg_enable = registers.lcdc.contains(LCDControl::BG_WINDOW_ENABLE);

        let window_enable = registers.lcdc.contains(LCDControl::WINDOW_ENABLE) && registers.wx < 167 && registers.wy < 144;
        let is_window_scanline = window_enable && registers.ly >= registers.wy && registers.ly < registers.wy.wrapping_add(144);

        let sprite_16 = registers.lcdc.contains(LCDControl::OBJ_SIZE);
        let sprite_height: u8 = if sprite_16 {
            16
        } else {
            8
        };

        match mode {
            0 => {
                if line_dot == 0 {
                    if registers.ly == 144 {
                        mode = 1;

                        if lcd_enable {
                            registers.interrupt_flag.insert(InterruptFlags::VBLANK);

                            // According to The Cycle-Accurate Game Boy Docs, OAM bit also triggers the interrupt on VBlank.
                            if registers.stat & (1 << 4) != 0 || registers.stat & (1 << 5) != 0 {
                                registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
                            }
                        }
                    } else {
                        mode = 2;

                        self.sprites.clear();

                        if lcd_enable && registers.stat & (1 << 5) != 0 {
                            registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
                        }
                    }
                }
            }
            1 => {
                if self.dot_counter == 0 {
                    mode = 2;

                    self.sprites.clear();
                }

                registers.ly = (self.dot_counter / 456) as u8;
            }
            2 => {
                self.fetch_sprites(registers, sprite_height, line_dot);

                if line_dot == 80 {
                    mode = 3;

                    self.pixel_fetcher.bg_fifo.clear();
                    self.pixel_fetcher.obj_fifo.clear();

                    self.sprites.sort_by(|a, b| match a.x.cmp(&b.x) {
                        Ordering::Equal => a.oam_index.cmp(&b.oam_index),
                        ord => ord
                    });
                }
            }
            3 => {
                self.pixel_fetcher.tick(&self.vram, &registers);

                if self.delay > 0 {
                    self.delay -= 1;

                    return false;
                }

                if self.pixel_fetcher.bg_fifo.is_empty() && matches!(self.pixel_fetcher.state, Sleep) {
                    self.fetch_bg_pixels(&registers, is_window_scanline);

                    return false;
                }

                let sprites_enable = registers.lcdc.contains(LCDControl::OBJ_ENABLE);
                if sprites_enable {
                    // TODO: The following is performed for each sprite on the current scanline if LCDC.1 is enabled and the X coordinate of the current scanline has a sprite on it. (?) 
                    //  If those conditions are not met then sprite fetching is aborted.

                    // At this point the fetcher is advanced one step until it's at step 5 or until the background FIFO is not empty. (Handled inside fetch_obj_pixels).
                    // Advancing the fetcher one step here lengthens mode 3 by 1 dot. This process may be aborted after the fetcher has advanced a step.
                    if self.sprites.iter().any(|s| self.scanline_x + 8 >= s.x && self.scanline_x < s.x) {
                        self.fetch_obj_pixels(&registers);

                        return false;
                    }

                    // After checking for sprites at X coordinate 0 the fetcher is advanced two steps. 
                    // The first advancement lengthens mode 3 by 1 dot and the second advancement lengthens mode 3 by 3 dots. 
                    // After each fetcher advancement there is a chance for a sprite fetch abortion to occur.

                    // The lower address for the row of pixels of the target object tile is now retrieved and lengthens mode 3 by 1 dot.
                    // Once the address is retrieved this is the last chance for sprite fetch abortion to occur. 
                    // Exiting object fetch lengthens mode 3 by 1 dot.
                    // The upper address for the target object tile is now retrieved and does not shorten mode 3.

                    // Before any mixing is done, if the OAM FIFO doesn't have at least 8 pixels in it then transparent pixels with the lowest priority are pushed onto the OAM FIFO.
                    // let mut i = 0;
                    // while self.pixel_fetcher.obj_fifo.len() < 8 {
                    //     self.pixel_fetcher.obj_fifo.push_back(SpritePixel {
                    //         x: self.scanline_x + i,
                    //         color: 0,
                    //         bg_over_obj: false,
                    //         palette: registers.obp0,
                    //     });
                    // 
                    //     i += 1;
                    // }
                }

                let bg_pixel = self.pixel_fetcher.bg_fifo.pop_front();

                let skip = registers.scx % 8;
                if self.scanline_x == 0 && skip > 0 {
                    if self.pixel_fetcher.bg_fifo.len() >= (8 - skip) as usize {
                        return false;
                    }
                }

                // When SCX & 7 > 0 and there is a sprite at X coordinate 0 of the current scanline then mode 3 is lengthened.
                // The amount of dots this lengthens mode 3 by is whatever the lower 3 bits of SCX are. 
                // TODO: After this penalty is applied object fetching may be aborted.
                // if self.scanline_x == 0 && registers.scx & 7 > 0 && self.sprites.iter().any(|s| s.x == 0) {
                //     self.delay += (registers.scx & 7) as usize;
                // 
                //     return false;
                // }

                // Now it's time to render a pixel!
                // The same process described in Sprite Fetch Abortion is performed: a pixel is rendered and the fetcher is advanced one step.
                // This advancement lengthens mode 3 by 1 dot if the X coordinate of the current scanline is not 160.
                // If the X coordinate is 160 the PPU stops processing sprites (because they won't be visible).

                if bg_pixel.is_none() {
                    return false;
                }

                let bg_pixel = bg_pixel.unwrap();

                let mut pixel = 0;
                let mut palette = registers.bgp;

                if bg_enable {
                    pixel = bg_pixel;
                }

                while let Some(sprite_pixel) = self.pixel_fetcher.obj_fifo.pop_front() {
                    if sprite_pixel.x < self.scanline_x as isize { 
                        continue;
                    }
                    
                    if !bg_enable {
                        pixel = sprite_pixel.color;
                        palette = sprite_pixel.palette;
                    } else if sprites_enable {
                        if sprite_pixel.bg_over_obj && bg_pixel != 0 || sprite_pixel.color == 0 {
                            pixel = bg_pixel;
                        } else {
                            pixel = sprite_pixel.color;
                            palette = sprite_pixel.palette;
                        }
                    }
                    
                    break;
                }

                if self.scanline_x < 160 && registers.ly < 144 {
                    let color = (palette >> (pixel * 2)) & 0b0000_0011;

                    self.screen[registers.ly as usize * 160 + self.scanline_x as usize] = color;

                    self.scanline_x = (self.scanline_x + 1) % 160;

                    if self.scanline_x == 0 {
                        mode = 0;

                        if lcd_enable && registers.stat & (1 << 3) != 0 {
                            registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
                        }

                        if lcd_enable {
                            if is_window_scanline {
                                registers.window_ly = (registers.window_ly + 1) % 144;
                            }

                            registers.ly += 1;
                        }

                        // result = true;
                    }
                }
            }
            _ => unreachable!()
        }

        if lcd_enable {
            self.dot_counter += 1;
        }

        if self.dot_counter == 70224 {
            self.dot_counter = 0;

            result = true;

            registers.ly = 0;
            registers.window_ly = 0;

            registers.interrupt_flag.remove(InterruptFlags::VBLANK | InterruptFlags::LCD_STAT);
        }

        registers.stat = (registers.stat & 0b1111_1100) | (mode & 0b0000_0011);

        return result;
    }

    fn fetch_sprites(&mut self, registers: &mut IoRegisters, sprite_height: u8, line_dot: usize) {
        if line_dot % 2 == 0 {
            return;
        }

        if self.sprites.len() == self.sprites.capacity() {
            return;
        }

        let ly = registers.ly;

        let oam_addr = 0xfe00 + (line_dot as u16 / 2) * 4;
        let sprite_y = self.vram.mem_read(oam_addr);

        if sprite_y <= ly + 16 && ly + 16 - sprite_height < sprite_y {
            self.sprites.push(Oam {
                oam_index: line_dot as u8 / 2,
                y: sprite_y,
                x: self.vram.mem_read(oam_addr + 1),
                tile_index: self.vram.mem_read(oam_addr + 2),
                attributes: self.vram.mem_read(oam_addr + 3),
            });
        }
    }
    fn fetch_bg_pixels(&mut self, registers: &IoRegisters, is_window_scanline: bool) {
        let is_window_tile = is_window_scanline && (self.scanline_x + 7) >= registers.wx && (self.scanline_x + 7) <= registers.wx.saturating_add(159);

        let tile_map_addr = if is_window_tile {
            let bit_10: u16 = if registers.lcdc.contains(LCDControl::WINDOW_TILEMAP_AREA) { 1 } else { 0 };
            let offset_y = registers.window_ly as u16 / 8;
            let offset_x = (self.scanline_x + 7).wrapping_sub(registers.wx) as u16 / 8;

            0b1001_1000_0000_0000 | (bit_10 << 10) | offset_y << 5 | offset_x
        } else {
            let bit_10: u16 = if registers.lcdc.contains(LCDControl::BG_TILEMAP_AREA) { 1 } else { 0 };
            let offset_y = registers.ly.wrapping_add(registers.scy) as u16 / 8;
            let offset_x = self.scanline_x.wrapping_add(registers.scx) as u16 / 8;

            0b1001_1000_0000_0000 | (bit_10 << 10) | offset_y << 5 | offset_x
        };

        if is_window_tile {
            self.pixel_fetcher.fetch_window_tile(tile_map_addr);

            // When WX is 0 and the SCX & 7 > 0 mode 3 is shortened by 1 dot.
            if registers.wx == 0 && registers.scx & 7 > 0 {
                // self.target_mode3_count -= 1;
            }
        } else {
            self.pixel_fetcher.fetch_bg_tile(tile_map_addr);
        }
    }

    fn fetch_obj_pixels(&mut self, registers: &IoRegisters) {
        if self.pixel_fetcher.bg_fifo.is_empty() || !matches!(self.pixel_fetcher.state, Sleep) {
            return;
        }

        let sprite_16 = registers.lcdc.contains(LCDControl::OBJ_SIZE);

        if let Some(index) = self.sprites.iter().position(|s| self.scanline_x + 8 >= s.x && self.scanline_x < s.x) {
            let mut sprite = self.sprites.remove(index);

            let flip_sprite_v = sprite.attributes & (1 << 6) != 0;

            sprite.tile_index = if sprite_16 {
                match (flip_sprite_v, registers.ly + 16 - sprite.y < 8) {
                    (true, true) | (false, false) => sprite.tile_index | 0x01,
                    _ => sprite.tile_index & 0xfe,
                }
            } else {
                sprite.tile_index
            };

            self.pixel_fetcher.fetch_obj_tile(&sprite);

            self.delay += 7;
        }
    }
}
