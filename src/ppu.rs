use std::cmp::Ordering;
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

fn fetch_tile_bytes(vram: &Vram, registers: &IoRegisters, tile_index: u8, pixel_offset_y: u8) -> (u8, u8) {
    let tile_data_area = registers.lcdc & (1 << 4) != 0;
    let tile_vram_addr: u16 = match (tile_data_area, tile_index) {
        (true, _) => 0x8000 + (tile_index as u16 * 16),
        (false, 0..=127) => 0x9000 + (tile_index as u16 * 16),
        (false, 128..=255) => 0x8800 + (tile_index as u16 * 16),
        _ => unreachable!()
    };

    let tile_vram_y_offset = pixel_offset_y as u16 % 8;
    let tile_byte_lo = vram.mem_read(tile_vram_addr + tile_vram_y_offset * 2 + 0);
    let tile_byte_hi = vram.mem_read(tile_vram_addr + tile_vram_y_offset * 2 + 1);

    (tile_byte_lo, tile_byte_hi)
}

struct PixelFetcher {
    counter: usize,
    fetcher_x: u8,
    current_tile_x: u8,
    current_tile_row: u8,
    current_tile_byte_lo: u8,
    current_tile_byte_hi: u8,
    pub current_step: usize,
    pub bg_fifo: VecDeque<FifoEntry>,

    tiles: [u8; 32 * 32],
}

impl PixelFetcher {
    pub fn new() -> Self {
        PixelFetcher {
            counter: 0,
            current_step: 0,
            fetcher_x: 0,
            current_tile_x: 0,
            current_tile_row: 0,
            current_tile_byte_lo: 0,
            current_tile_byte_hi: 0,
            bg_fifo: VecDeque::with_capacity(16),

            tiles: [255; 32 * 32],
        }
    }

    pub fn step(&mut self, vram: &Vram, registers: &IoRegisters, scanline_x: u8) -> bool {
        self.counter = (self.counter + 1) % 2;

        let window_enable = registers.lcdc & (1 << 5) != 0;

        let wx = registers.wx as usize;
        let window_span_x = wx..=wx.saturating_add(160);
        let is_window_tile = window_enable & window_span_x.contains(&(scanline_x as usize));

        match self.current_step {
            0 => {
                // Step takes 2 dots.
                if self.counter == 1 {
                    return false;
                }

                let (tile_offset_x, tile_pixel_offset_y) = if is_window_tile {
                    (
                        self.fetcher_x,
                        registers.ly.wrapping_sub(registers.wy)
                    )
                } else {
                    (
                        ((registers.scx / 8) + self.fetcher_x) & 0x1f,
                        registers.ly.wrapping_add(registers.scy)
                    )
                };

                self.current_tile_x = tile_offset_x;
                self.current_tile_row = tile_pixel_offset_y;

                self.current_step += 1;
            }
            1 => {
                // Step takes 2 dots.
                if self.counter == 1 {
                    return false;
                }

                let mut tile_map_addr: u16 = 0x9800;

                // When LCDC.3 is enabled and the X coordinate of the current scanline is not inside the window then tilemap $9C00 is used.
                // When LCDC.6 is enabled and the X coordinate of the current scanline is inside the window then tilemap $9C00 is used.
                if registers.lcdc & (1 << 3) != 0 && !is_window_tile ||
                    registers.lcdc & (1 << 6) != 0 && is_window_tile {
                    tile_map_addr = 0x9c00;
                }

                let tile_index = vram.mem_read(tile_map_addr + self.current_tile_row as u16 / 8 * 32 + self.current_tile_x as u16);

                let (tile_byte_lo, _) = fetch_tile_bytes(vram, registers, tile_index, self.current_tile_row);

                self.current_tile_byte_lo = tile_byte_lo;

                self.current_step += 1;
            }
            2 => {
                // Step takes 2 dots.
                if self.counter == 1 {
                    return false;
                }

                // TODO: dedupe
                let mut tile_map_addr: u16 = 0x9800;

                // When LCDC.3 is enabled and the X coordinate of the current scanline is not inside the window then tilemap $9C00 is used.
                // When LCDC.6 is enabled and the X coordinate of the current scanline is inside the window then tilemap $9C00 is used.
                if registers.lcdc & (1 << 3) != 0 && !is_window_tile ||
                    registers.lcdc & (1 << 6) != 0 && is_window_tile {
                    tile_map_addr = 0x9c00;
                }

                let tile_index = vram.mem_read(tile_map_addr + self.current_tile_row as u16 / 8 * 32 + self.current_tile_x as u16);

                if tile_index != 32 {
                    println!("{} {} {:X}", self.current_tile_row, self.current_tile_x, tile_index);
                }

                let (_, tile_byte_hi) = fetch_tile_bytes(vram, registers, tile_index, self.current_tile_row);

                self.current_tile_byte_hi = tile_byte_hi;

                if self.bg_fifo.is_empty() && registers.ly < 144 && scanline_x < 160 {
                    self.push_pixels(registers.ly);
                }

                self.fetcher_x = (self.fetcher_x + 1) & 0x1f;

                self.current_step += 1;
            }
            3 => {
                // Sleep.
                // Step takes 2 dots.
                if self.counter == 1 {
                    return false;
                }

                self.current_step += 1;
            }
            4 => {
                if !self.bg_fifo.is_empty() {
                    return false;
                }

                if registers.ly < 144 && scanline_x < 160 {
                    self.push_pixels(registers.ly);
                }

                self.current_step = 0;
            }
            _ => unreachable!()
        }

        return true;
    }

    fn push_pixels(&mut self, y: u8) {
        for i in 0..=7 {
            let color = (((self.current_tile_byte_hi >> (7 - i)) & 0b0000_0001) << 1) | (self.current_tile_byte_lo >> (7 - i) & 0b0000_0001);

            let pixel = FifoEntry {
                x: self.current_tile_x * 8 + i,
                y,
                color,
            };
            self.bg_fifo.push_back(pixel);
        }
    }
}

struct FifoEntry {
    pub x: u8,
    pub y: u8,
    pub color: u8,
}

struct SpriteFifoEntry {
    pub x: u8,
    pub y: u8,
    pub color: u8,
    pub palette: u8,
    pub bg_over_obj: bool,
}

struct Oam {
    pub y: u8,
    pub x: u8,
    pub tile_index: u8,
    pub attributes: u8,
    oam_index: usize,
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
    fetcher_x: u8,
    pixel_fetcher: PixelFetcher,
    bg_fifo: VecDeque<FifoEntry>,
    sprite_fifo: VecDeque<SpriteFifoEntry>,
    pub screen: [u8; 160 * 144],
    window_on_current_line: bool,
}

impl Ppu {
    pub fn new() -> Ppu {
        Ppu {
            dot_counter: 0,
            vram: Vram::new(),
            scanline_x: 0,
            fetcher_x: 0,
            pixel_fetcher: PixelFetcher::new(),
            bg_fifo: VecDeque::with_capacity(16),
            sprite_fifo: VecDeque::with_capacity(16),
            screen: [0; 160 * 144],
            window_on_current_line: false,
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

            if line_dot == 0 {
                mode = 2;

                self.window_on_current_line = registers.wy == registers.ly;
            }

            if line_dot == 80 {
                mode = 3;

                self.scanline_x = 0;

                self.pixel_fetcher.bg_fifo.clear();
                self.pixel_fetcher.current_step = 0;
                self.pixel_fetcher.counter = 0;

                self.sprite_fifo.clear();

                if registers.wx == 0 && registers.scx & 7 > 0 {
                    // TODO: Shorten mode 3 by 1 dot.
                }
            }

            if mode == 3 {
                let sprites_enable = false; // registers.lcdc & (1 << 1) != 0;

                self.pixel_fetcher.step(&self.vram, &registers, self.scanline_x);

                // let window_enable = registers.lcdc & (1 << 5) != 0;
                // 
                // let wx = registers.wx as usize;
                // let window_span_x = wx..=(wx + 160);
                // let is_window_tile = window_enable & window_span_x.contains(&(self.scanline_x as usize));
                // 
                // let (tile_offset_x, tile_pixel_offset_y) = if is_window_tile {
                //     (
                //         self.fetcher_x,
                //         registers.ly.wrapping_sub(registers.wy)
                //     )
                // } else {
                //     (
                //         ((registers.scx / 8) + self.fetcher_x) & 0x1f,
                //         registers.ly.wrapping_add(registers.scy)
                //     )
                // };
                // 
                // self.fetcher_x = (self.fetcher_x + 1) & 0x1f;
                // 
                // let mut tile_map_addr: u16 = 0x9800;
                // 
                // // When LCDC.3 is enabled and the X coordinate of the current scanline is not inside the window then tilemap $9C00 is used.
                // // When LCDC.6 is enabled and the X coordinate of the current scanline is inside the window then tilemap $9C00 is used.
                // if registers.lcdc & (1 << 3) != 0 && !is_window_tile ||
                //     registers.lcdc & (1 << 6) != 0 && is_window_tile {
                //     tile_map_addr = 0x9c00;
                // }
                // 
                // let tile_index = self.vram.mem_read(tile_map_addr + (tile_pixel_offset_y / 8) as u16 * 32 + tile_offset_x as u16);
                // 
                // let tile_pixel_offset_y = if is_window_tile {
                //     registers.ly.wrapping_sub(registers.wy) % 8
                // } else {
                //     registers.ly.wrapping_add(registers.scy)
                // };
                // 
                // let (tile_byte_lo, tile_byte_hi) = fetch_tile_bytes(&self.vram, registers, tile_index, tile_pixel_offset_y);
                // 
                // if self.bg_fifo.is_empty() {
                //     for i in 0..=7 {
                //         let color = (((tile_byte_hi >> (7 - i)) & 0b0000_0001) << 1) | (tile_byte_lo >> (7 - i) & 0b0000_0001);
                // 
                //         let pixel = FifoEntry {
                //             x: tile_offset_x * 8 + i,
                //             y: tile_pixel_offset_y,
                //             color,
                //         };
                //         self.bg_fifo.push_back(pixel);
                //     }
                // }


                // The following is performed for each sprite on the current scanline if LCDC.1 is enabled 
                // (this condition is ignored on CGB) and the X coordinate of the current scanline has a sprite on it. 
                // If those conditions are not met then sprite fetching is aborted.
                if sprites_enable {
                    let sprite_16 = registers.lcdc & (1 << 2) != 0;
                    let sprite_height: u8 = if sprite_16 {
                        8
                    } else {
                        16
                    };

                    let mut sprites = self.fetch_sprites(&registers, sprite_height);

                    if sprites.iter().any(|s| s.x == self.scanline_x) {
                        sprites.sort_by(|a, b| match a.x.cmp(&b.x) {
                            Ordering::Equal => a.oam_index.cmp(&b.oam_index),
                            ord => ord
                        });

                        let mut extend_mode_by = 0;
                        for sprite in sprites {
                            if !(sprite.x.saturating_sub(7)..=sprite.x).contains(&self.scanline_x) {
                                continue;
                            }

                            loop {
                                if self.pixel_fetcher.current_step == 4 || self.pixel_fetcher.bg_fifo.is_empty() {
                                    break;
                                }

                                if self.pixel_fetcher.step(&self.vram, &registers, self.scanline_x) {
                                    extend_mode_by += 1;
                                }
                            }

                            // TODO: When SCX & 7 > 0 and there is a sprite at X coordinate 0 of the current scanline then mode 3 is lengthened.
                            //  The amount of dots this lengthens mode 3 by is whatever the lower 3 bits of SCX are. After this penalty is applied object fetching may be aborted.
                            if registers.scx & 7 > 0 && sprite.x == 0 {
                                let mut steps = 2;

                                // After checking for sprites at X coordinate 0 the fetcher is advanced two steps. 
                                // The first advancement lengthens mode 3 by 1 dot and the second advancement lengthens mode 3 by 3 dots. 
                                // TODO: After each fetcher advancement there is a chance for a sprite fetch abortion to occur.
                                while steps > 0 {
                                    if self.pixel_fetcher.step(&self.vram, &registers, self.scanline_x) {
                                        steps -= 1;
                                    }
                                }

                                extend_mode_by += 4;
                            }

                            let sprite_pixel_row = sprite.y - registers.ly + sprite_height - 16;

                            let tile_index = match (sprite_16, sprite_pixel_row) {
                                (false, 0..=7) => sprite.tile_index,
                                (true, 0..=7) => sprite.tile_index & 0xfe,
                                (true, 8..=15) => sprite.tile_index | 0x01,
                                _ => unreachable!(),
                            };

                            let sprite_tile_byte_lo_addr = self.vram.mem_read(0x8000 + tile_index as u16 * 16 + 0);

                            extend_mode_by += 1;

                            let sprite_tile_byte_hi_addr = self.vram.mem_read(0x8000 + tile_index as u16 * 16 + 1);

                            if self.sprite_fifo.is_empty() {
                                for i in 0..=7 {
                                    self.sprite_fifo.push_back(SpriteFifoEntry {
                                        x: sprite.x + i,
                                        y: sprite.y,
                                        color: 0,
                                        bg_over_obj: false,
                                        palette: registers.obp0,
                                    });
                                }
                            }

                            for i in 0..=7 {
                                let color = (((sprite_tile_byte_hi_addr >> (7 - i)) & 0b0000_0001) << 1) | (sprite_tile_byte_lo_addr >> (7 - i) & 0b0000_0001);

                                let fifo_entry = &mut self.sprite_fifo[7 - i];
                                if fifo_entry.color == 0 && color != 0 {
                                    fifo_entry.color = color;
                                    fifo_entry.bg_over_obj = sprite.attributes & (1 << 7) != 0;
                                    fifo_entry.palette = if sprite.attributes & (1 << 4) == 0 {
                                        registers.obp0
                                    } else {
                                        registers.obp1
                                    };
                                }
                            }
                        }
                    }
                }

                let bg_enable = registers.lcdc & 1 != 0;

                let sprite_pixel = self.sprite_fifo.pop_front();
                let bg_pixel = self.pixel_fetcher.bg_fifo.pop_front();

                let mut final_pixel: Option<u8> = None;

                if let Some(bg_pixel) = bg_pixel {
                    final_pixel = if bg_enable {
                        Some(registers.bgp >> (bg_pixel.color * 2) & 0b0000_0011)
                    } else {
                        Some(0)
                    };

                    if let Some(sprite_pixel) = sprite_pixel {
                        if sprites_enable && sprite_pixel.color != 0 && !sprite_pixel.bg_over_obj {
                            final_pixel = Some((sprite_pixel.palette >> (sprite_pixel.color * 2)) & 0b0000_0011)
                        }
                    }
                };

                if let Some(pixel) = final_pixel {
                    if registers.ly < 144 {
                        self.screen[registers.ly as usize * 160 + self.scanline_x as usize] = 255 - pixel * 64;

                        self.scanline_x = (self.scanline_x + 1) % 160;

                        if self.scanline_x == 0 {
                            registers.stat = registers.stat & 0b1111_0111 | 0b0000_1000;

                            registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
                        }
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

    fn fetch_sprites(&mut self, registers: &&mut IoRegisters, sprite_height: u8) -> Vec<Oam> {
        let mut sprites: Vec<Oam> = Vec::with_capacity(10);

        for (index, sprite_addr) in (0xfe00..=0xfe9f).step_by(4).enumerate() {
            let sprite_y = self.vram.mem_read(sprite_addr);
            let sprite_span = (sprite_y as isize - 16)..=(sprite_y as isize - 16 + sprite_height as isize);

            if sprite_span.contains(&(registers.ly as isize)) {
                sprites.push(Oam {
                    oam_index: index,
                    y: sprite_y,
                    x: self.vram.mem_read(sprite_addr + 1),
                    tile_index: self.vram.mem_read(sprite_addr + 2),
                    attributes: self.vram.mem_read(sprite_addr + 3),
                });
            }

            if sprites.len() == sprites.capacity() {
                break;
            }
        }

        return sprites;
    }
}
