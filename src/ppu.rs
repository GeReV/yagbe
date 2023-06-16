use std::cmp::Ordering;
use bitflags::Flags;

use crate::io_registers::{InterruptFlags, IoRegisters, LCDControl};
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
    let tile_data_area = registers.lcdc.contains(LCDControl::BG_TILEDATA_AREA);
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

struct SpritePixel {
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
    pub screen: [u8; 160 * 144],
}

impl Ppu {
    pub fn new() -> Ppu {
        Ppu {
            dot_counter: 0,
            vram: Vram::new(),
            scanline_x: 0,
            screen: [0; 160 * 144],
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

        if registers.ly >= 144 {
            if registers.ly == 144 {
                mode = 1;

                if lcd_enable {
                    registers.interrupt_flag.insert(InterruptFlags::VBLANK);

                    // According to The Cycle-Accurate Game Boy Docs, OAM bit also triggers the interrupt on VBlank.
                    if registers.stat & (1 << 4) != 0 || registers.stat & (1 << 5) != 0 {
                        registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
                    }
                }
            }
        } else {
            // TODO: This should be used to take into account at which step we are.
            let line_dot = self.dot_counter % 456;

            let window_enable = registers.lcdc.contains(LCDControl::WINDOW_ENABLE) && registers.wx < 167 && registers.wy < 144;
            let is_window_scanline = window_enable && registers.ly >= registers.wy && registers.ly < registers.wy.wrapping_add(144);

            if line_dot == 0 {
                mode = 2;

                if lcd_enable && registers.stat & (1 << 5) != 0 {
                    registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
                }
            }

            if line_dot == 80 {
                mode = 3;
            }

            if mode == 3 {
                let sprites_enable = registers.lcdc.contains(LCDControl::OBJ_ENABLE);
                
                let is_window_tile = is_window_scanline && (self.scanline_x + 7) >= registers.wx && (self.scanline_x + 7) <= registers.wx.saturating_add(159);

                let (tile_offset_x, tile_pixel_offset_y) = if is_window_tile {
                    (
                        (self.scanline_x + 7).wrapping_sub(registers.wx) / 8,
                        registers.window_ly
                    )
                } else {
                    (
                        ((registers.scx / 8) + self.scanline_x / 8) & 0x1f,
                        registers.ly.wrapping_add(registers.scy)
                    )
                };

                let mut tile_map_addr: u16 = 0x9800;

                // When LCDC.3 is enabled and the X coordinate of the current scanline is not inside the window then tilemap $9C00 is used.
                // When LCDC.6 is enabled and the X coordinate of the current scanline is inside the window then tilemap $9C00 is used.
                if registers.lcdc.contains(LCDControl::BG_TILEMAP_AREA) && !is_window_tile ||
                    registers.lcdc.contains(LCDControl::WINDOW_TILEMAP_AREA) && is_window_tile {
                    tile_map_addr = 0x9c00;
                }

                let tile_index = self.vram.mem_read(tile_map_addr + (tile_pixel_offset_y / 8) as u16 * 32 + tile_offset_x as u16);

                let (tile_byte_lo, tile_byte_hi) = fetch_tile_bytes(&self.vram, registers, tile_index, tile_pixel_offset_y);

                let pixel_offset = if is_window_tile {
                    (self.scanline_x + 7).wrapping_sub(registers.wx) % 8
                } else {
                    registers.scx.wrapping_add(self.scanline_x) % 8
                };

                let bg_pixel = (((tile_byte_hi >> (7 - pixel_offset)) & 1) << 1) | (tile_byte_lo >> (7 - pixel_offset) & 1);

                // The following is performed for each sprite on the current scanline if LCDC.1 is enabled 
                // (this condition is ignored on CGB) and the X coordinate of the current scanline has a sprite on it. 
                // If those conditions are not met then sprite fetching is aborted.
                let mut sprite_pixel: Option<SpritePixel> = None;
                if sprites_enable {
                    let sprite_16 = registers.lcdc.contains(LCDControl::OBJ_SIZE);
                    let sprite_height: u8 = if sprite_16 {
                        16
                    } else {
                        8
                    };

                    let mut sprites = self.fetch_sprites(&registers, sprite_height);

                    if sprites.iter().any(|s| self.scanline_x + 7 >= s.x && self.scanline_x <= s.x) {
                        sprites.sort_by(|a, b| match a.x.cmp(&b.x) {
                            Ordering::Equal => a.oam_index.cmp(&b.oam_index),
                            ord => ord
                        });

                        for sprite in sprites {
                            if self.scanline_x + 7 < sprite.x || self.scanline_x > sprite.x {
                                continue;
                            }

                            let flip_sprite_v = sprite.attributes & (1 << 6) != 0;
                            let flip_sprite_h = sprite.attributes & (1 << 5) != 0;
                            
                            let sprite_y_offset = sprite.y - registers.ly + sprite_height - 16;
                            let sprite_tile_offset = sprite_y_offset / 8;

                            let tile_index = match (sprite_16, flip_sprite_v, sprite_tile_offset) {
                                (false, _, 0) => sprite.tile_index,
                                (true, true, 0) | (true, false, 1) => sprite.tile_index & 0xfe,
                                (true, true, 1) | (true, false, 0) => sprite.tile_index | 0x01,
                                _ => unreachable!(),
                            };

                            let tile_row_offset = sprite_y_offset % 8;
                            
                            let tile_row_offset = if flip_sprite_v {
                                tile_row_offset
                            } else {
                                7 - tile_row_offset
                            };

                            let sprite_tile_byte_lo_addr = self.vram.mem_read(0x8000 + tile_index as u16 * 16 + tile_row_offset as u16 * 2 + 0);
                            let sprite_tile_byte_hi_addr = self.vram.mem_read(0x8000 + tile_index as u16 * 16 + tile_row_offset as u16 * 2 + 1);

                            let pixel_offset = self.scanline_x + 7 - sprite.x;
                            let pixel_offset = if flip_sprite_h {
                                pixel_offset
                            } else {
                                7 - pixel_offset
                            };

                            let color = (((sprite_tile_byte_hi_addr >> pixel_offset) & 1) << 1) | (sprite_tile_byte_lo_addr >> pixel_offset & 1);

                            if color == 0 {
                                continue;
                            }

                            sprite_pixel = Some(SpritePixel {
                                color,
                                bg_over_obj: sprite.attributes & (1 << 7) != 0,
                                palette: if sprite.attributes & (1 << 4) == 0 {
                                    registers.obp0
                                } else {
                                    registers.obp1
                                },
                            });

                            break;
                        }
                    }
                }

                let bg_enable = registers.lcdc.contains(LCDControl::BG_WINDOW_ENABLE);

                let mut final_pixel = registers.bgp & 0b0000_0011;

                if lcd_enable && bg_enable {
                    final_pixel = registers.bgp >> (bg_pixel * 2) & 0b0000_0011;
                }

                if let Some(sprite_pixel) = sprite_pixel {
                    if lcd_enable && sprites_enable && sprite_pixel.color != 0 && !sprite_pixel.bg_over_obj {
                        final_pixel = (sprite_pixel.palette >> (sprite_pixel.color * 2)) & 0b0000_0011;
                    }
                }

                if self.scanline_x < 160 {
                    self.screen[registers.ly as usize * 160 + self.scanline_x as usize] = 255 - final_pixel * 64;

                    self.scanline_x = (self.scanline_x + 1) % 160;

                    if self.scanline_x == 0 {
                        mode = 0;

                        // result = true;

                        if lcd_enable && registers.stat & (1 << 3) != 0 {
                            registers.interrupt_flag.insert(InterruptFlags::LCD_STAT);
                        }

                        if lcd_enable {
                            // println!("{window_enable} {} {}", registers.ly, self.window_ly);
                            
                            if is_window_scanline {
                                registers.window_ly = (registers.window_ly + 1) % 144;
                            }
                            
                            registers.ly += 1;
                        }
                    }
                }
            }
        }

        self.dot_counter += 1;

        if self.dot_counter == 70224 {
            self.dot_counter = 0;
            
            registers.ly = 0;
            registers.window_ly = 0;

            registers.interrupt_flag.remove(InterruptFlags::VBLANK | InterruptFlags::LCD_STAT);

            result = true;
        }

        registers.stat = (registers.stat & 0b1111_1100) | (mode & 0b0000_0011);

        return result;
    }

    fn fetch_sprites(&mut self, registers: &&mut IoRegisters, sprite_height: u8) -> Vec<Oam> {
        let mut sprites: Vec<Oam> = Vec::with_capacity(10);

        for (index, sprite_addr) in (0xfe00..=0xfe9f).step_by(4).enumerate() {
            let sprite_y = self.vram.mem_read(sprite_addr);

            let y = registers.ly as isize;
            if (sprite_y as isize - 16) < y && y <= (sprite_y as isize - 16 + sprite_height as isize) {
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
