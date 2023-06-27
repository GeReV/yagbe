use std::collections::VecDeque;
use bitflags::Flags;
use crate::io_registers::{IoRegisters, LCDControl};
use crate::Mem;
use crate::pixel_fetcher::PixelFetcherMode::{Background, Object};
use crate::pixel_fetcher::PixelFetcherState::{GetSpriteAttributes, GetTileId, GetTileRowHigh, GetTileRowLow, PushPixels};
use crate::ppu::{Oam, Vram};

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

pub enum PixelFetcherState {
    GetTileId,
    GetSpriteAttributes {
        tile_index: u8,
    },
    GetTileRowLow {
        sprite_attributes: Option<u8>,
        tile_index: u8,
    },
    GetTileRowHigh {
        sprite_attributes: Option<u8>,
        tile_address: u16,
        tile_byte_lo: u8,
    },
    PushPixels {
        sprite_attributes: Option<u8>,
        tile_byte_lo: u8,
        tile_byte_hi: u8,
    },
}

pub enum PixelFetcherMode {
    Background,
    Object {
        oam: Oam,
        sprite_offset: u8,
    },
}

pub struct SpritePixel {
    pub x: isize,
    pub color: u8,
    pub palette: u8,
    pub bg_over_obj: bool,
}

pub struct BgPixel {
    pub x: isize,
    pub color: u8,
}

pub struct PixelFetcher {
    dot_counter: usize,
    current_tile_map_line_addr: u16,
    current_tile_index: u8,
    current_tile_row_offset: u8,
    state: PixelFetcherState,
    pub mode: PixelFetcherMode,
    pub bg_fifo: VecDeque<BgPixel>,
    pub obj_fifo: VecDeque<SpritePixel>,
}

impl PixelFetcher {
    pub fn new() -> Self {
        Self {
            dot_counter: 2,
            current_tile_map_line_addr: 0x9800,
            current_tile_index: 0,
            current_tile_row_offset: 0,
            state: GetTileId,
            mode: Background,
            bg_fifo: VecDeque::with_capacity(16),
            obj_fifo: VecDeque::with_capacity(8),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bg_fifo.len() <= 8
    }

    pub fn clear(&mut self) {
        self.state = GetTileId;

        self.bg_fifo.clear();
        self.obj_fifo.clear();
    }

    pub fn tick(&mut self, vram: &Vram, registers: &IoRegisters) {
        self.dot_counter -= 1;
        if self.dot_counter == 0 {
            self.dot_counter = 2;
        } else {
            return;
        }

        match self.state {
            GetTileId => {
                let tile_index = match self.mode {
                    Background => vram.mem_read(self.current_tile_map_line_addr + self.current_tile_index as u16),
                    Object { ref oam, .. } => {
                        vram.mem_read(oam.oam_addr + 2)
                    }
                };

                self.state = if matches!(self.mode, Object {..}) {
                    GetSpriteAttributes {
                        tile_index,
                    }
                } else {
                    GetTileRowLow {
                        sprite_attributes: None,
                        tile_index,
                    }
                };
            }
            GetSpriteAttributes { tile_index } => {
                if let Object { ref oam, .. } = self.mode {
                    let sprite_16 = registers.lcdc.contains(LCDControl::OBJ_SIZE);

                    let attributes = vram.mem_read(oam.oam_addr + 3);

                    let flip_sprite_v = attributes & (1 << 6) != 0;

                    let tile_index = if sprite_16 {
                        match (flip_sprite_v, registers.ly + 16 - oam.y < 8) {
                            (true, true) | (false, false) => tile_index | 0x01,
                            _ => tile_index & 0xfe,
                        }
                    } else {
                        tile_index
                    };

                    self.state = GetTileRowLow {
                        sprite_attributes: Some(attributes),
                        tile_index,
                    };
                } else {
                    unreachable!();
                }
            }
            GetTileRowLow { tile_index, sprite_attributes } => {
                let tile_index = tile_index as u16;

                let tile_address = match self.mode {
                    Background { .. } => {
                        // https://github.com/gbdev/pandocs/blob/bbdc0ef79ba46dcc8183ad788b651ae25b52091d/src/Rendering_Internals.md#get-tile-row-low
                        // For BG/Window tiles, bit 12 depends on LCDC bit 4. If that bit is set ("$8000 mode"), then bit 12 is always 0; otherwise ("$8800 mode"), it is the negation of the tile ID's bit 7. 
                        // The full logical formula is thus: !((LCDC & $10) || (tileID & $80)) (see gate VUZA in the schematics).
                        let bit_12 = !(registers.lcdc.contains(LCDControl::BG_TILEDATA_AREA) || (tile_index & (1 << 7) != 0));
                        let bit_12: u16 = if bit_12 { 1 } else { 0 };

                        0x8000 | (bit_12 << 12) | tile_index << 4 | (self.current_tile_row_offset as u16) << 1
                    }
                    Object { ref oam, .. } => {
                        let mut row_offset = registers.ly.wrapping_sub(oam.y % 8) % 8;

                        let flip_sprite_v = sprite_attributes.unwrap() & (1 << 6) != 0;

                        if flip_sprite_v {
                            row_offset = 7 - row_offset;
                        }

                        0x8000 | tile_index << 4 | (row_offset << 1) as u16
                    }
                };

                let tile_byte_lo = vram.mem_read(tile_address);

                self.state = GetTileRowHigh {
                    tile_byte_lo,
                    tile_address,
                    sprite_attributes,
                };
            }
            GetTileRowHigh { tile_byte_lo, tile_address, sprite_attributes } => {
                let tile_byte_hi = vram.mem_read(tile_address + 1);

                if matches!(self.mode, Background) && self.push_pixels(registers, tile_byte_lo, tile_byte_hi, sprite_attributes) {
                    self.state = GetTileId;
                    self.current_tile_index = (self.current_tile_index + 1) % 32;

                    return;
                }

                self.state = PushPixels {
                    tile_byte_lo,
                    tile_byte_hi,
                    sprite_attributes,
                };
            }
            PushPixels { tile_byte_lo, tile_byte_hi, sprite_attributes } => {
                if self.push_pixels(registers, tile_byte_lo, tile_byte_hi, sprite_attributes) {
                    if matches!(self.mode, Background) {
                        self.current_tile_index = (self.current_tile_index + 1) % 32;
                    }

                    self.state = GetTileId;
                    self.mode = Background;
                }
            }
        }
    }

    fn push_pixels(&mut self, registers: &IoRegisters, tile_byte_lo: u8, tile_byte_hi: u8, sprite_attributes: Option<u8>) -> bool {
        if let Object { oam: Oam { x, .. }, sprite_offset } = self.mode {
            let attributes = sprite_attributes.unwrap();

            let mut insert_pixel = |color: u8, i: u8| {
                let x = x as isize - 8 + i as isize;

                let j = i - sprite_offset;

                if self.obj_fifo.get(j as usize).is_some() {
                    return;
                }

                self.obj_fifo.push_back(
                    SpritePixel {
                        x,
                        color,
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
                for i in sprite_offset..=7 {
                    let pixel = (((tile_byte_hi >> i) & 1) << 1) | (tile_byte_lo >> i & 1);

                    insert_pixel(pixel, i);
                }
            } else {
                for i in sprite_offset..=7 {
                    let pixel = (((tile_byte_hi >> (7 - i)) & 1) << 1) | (tile_byte_lo >> (7 - i) & 1);

                    insert_pixel(pixel, i);
                }
            }

            return true;
        }

        if self.is_empty() {
            for i in 0..=7 {
                let color = (((tile_byte_hi >> (7 - i)) & 1) << 1) | (tile_byte_lo >> (7 - i) & 1);

                let x = if let Background = self.mode {
                    self.current_tile_index * 8
                } else { 0 };

                self.bg_fifo.push_back(BgPixel {
                    x: x as isize + i as isize,
                    color,
                });
            }

            return true;
        }

        return false;
    }

    pub fn fetch_bg_tile(&mut self, tile_map_line_addr: u16, tile_x: u8, tile_row_offset: u8) {
        self.dot_counter = 2;
        self.current_tile_map_line_addr = tile_map_line_addr;
        self.current_tile_index = tile_x;
        self.current_tile_row_offset = tile_row_offset;
        self.state = GetTileId;
        self.mode = Background;

        self.bg_fifo.clear();
    }

    pub fn fetch_obj_tile(&mut self, oam: Oam, sprite_offset: u8) {
        self.state = GetTileId;
        self.mode = Object {
            oam,
            sprite_offset,
        };
    }
}
