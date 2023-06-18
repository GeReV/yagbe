﻿use crate::apu::Apu;
use crate::bus::BankingMode::{AdvancedRomOrRamBanking, Simple};
use crate::io_registers::IoRegisters;
use crate::Mem;
use crate::ppu::Ppu;



const OFFSET_CARTRIDGE_TYPE: usize = 0x0147;
const OFFSET_ROM_SIZE: usize = 0x0148;
const OFFSET_RAM_SIZE: usize = 0x0149;

pub fn cartridge_ram_size_kib(ram_size_type: u8) -> usize {
    match ram_size_type {
        0 => 0,
        2 => 8,
        3 => 32,
        4 => 128,
        5 => 64,
        _ => unreachable!(),
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum BankingMode {
    // 00 = Simple Banking Mode (default)
    //      0000–3FFF and A000–BFFF locked to bank 0 of ROM/RAM
    Simple,
    // 01 = RAM Banking Mode / Advanced ROM Banking Mode
    //      0000–3FFF and A000–BFFF can be bank-switched via the 4000–5FFF bank register
    AdvancedRomOrRamBanking
}

pub struct Bus {
    program: Vec<u8>,
    pub bank0: [u8; 0x4000],
    pub bank1: [u8; 0x4000],
    pub banking_mode: BankingMode,
    pub cartridge_ram_size_type: u8,
    pub ram_enable: bool,
    pub ram_current_bank: u8,
    pub ram_banks: Vec<[u8; 0x2000]>,
    pub wram: [u8; 0x2000],
    pub hram: [u8; 0x7f],
    pub ppu: Ppu,
    pub apu: Apu,
    pub io_registers: IoRegisters,
}

impl Bus {
    pub fn new() -> Self {
        Bus {
            program: Vec::new(),
            bank0: [0; 0x4000],
            bank1: [0; 0x4000],
            banking_mode: Simple,
            cartridge_ram_size_type: 0,
            ram_enable: false,
            ram_current_bank: 0,
            ram_banks: Vec::with_capacity(0),
            wram: [0; 0x2000],
            hram: [0; 0x7f],
            ppu: Ppu::new(),
            apu: Apu::new(),
            io_registers: IoRegisters::new(),
        }
    }

    pub fn load(&mut self, program: Vec<u8>) {
        let checksum = self.verify_checksum(&program);

        let cartridge_type = program[OFFSET_CARTRIDGE_TYPE];

        let rom_size_type = program[OFFSET_ROM_SIZE];
        let rom_size_bytes: usize = 32 * 1024 * (1 << rom_size_type);

        self.cartridge_ram_size_type = program[OFFSET_RAM_SIZE];
        
        let cartridge_ram_bytes_total = cartridge_ram_size_kib(self.cartridge_ram_size_type) * 1024;
        
        self.ram_banks = Vec::with_capacity(cartridge_ram_bytes_total / 0x2000);
        self.ram_banks.fill([0; 0x2000]);

        self.bank0.copy_from_slice(&program[0x0000..=0x3fff]);
        self.bank1.copy_from_slice(&program[0x4000..=0x7fff]);

        self.program = program;
    }

    fn verify_checksum(&self, program: &Vec<u8>) -> bool {
        let mut checksum: u8 = 0;

        for i in 0x0134..=0x014c_usize {
            checksum = checksum.wrapping_sub(program[i]).wrapping_sub(1);
        }

        return checksum == program[0x014d];
    }
}

impl Mem for Bus {
    fn mem_read(&self, addr: u16) -> u8 {
        return match addr {
            0x0000..=0x3fff => self.bank0[addr as usize],
            0x4000..=0x7fff => self.bank1[(addr - 0x4000) as usize],
            0x8000..=0x9fff => self.ppu.vram.mem_read(addr),
            0xa000..=0xbfff => {
                let addr = (addr - 0xa000) as usize;
                
                match (self.ram_enable, self.banking_mode) {
                    (false, _) => 0xff,
                    (_, Simple) => self.ram_banks[0][addr],
                    (_, AdvancedRomOrRamBanking) => self.ram_banks[self.ram_current_bank as usize][addr]
                }
            },
            0xc000..=0xdfff => self.wram[(addr - 0xc000) as usize],
            0xe000..=0xfdff => self.wram[(addr - 0xe000) as usize],
            0xfe00..=0xfe9f => 0,
            0xfea0..=0xfeff => {
                // TODO: If OAM blocked
                // TODO: OAM corruption, return 0?
                return 0xff;
            }
            0xff10..=0xff26 => self.apu.mem_read(addr),
            0xff00..=0xff0f | 0xff30..=0xff7f => self.io_registers.mem_read(addr),
            0xff80..=0xfffe => self.hram[(addr - 0xff80) as usize],
            0xffff => self.io_registers.mem_read(addr),
            _ => unreachable!()
        };
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1fff => {
                self.ram_enable = value & 0x0f == 0x0a;
            }
            0x2000..=0x3fff => {
                let rom_size_type = self.program[OFFSET_ROM_SIZE];

                let bank_count_mask = match rom_size_type {
                    0x00 => 0b0000_0001,
                    0x01 => 0b0000_0011,
                    0x02 => 0b0000_0111,
                    0x03 => 0b0000_1111,
                    0x04 => 0b0001_1111,
                    // TODO: The rest take 2 bits from a different register.
                    _ => unreachable!()
                };

                let mut bank = value & bank_count_mask;

                let allow_bank0_mirroring = bank & 0b0001_0000 != 0;

                if bank == 0x00 {
                    bank = if allow_bank0_mirroring {
                        0x00
                    } else { 
                        0x01
                    };
                }

                let bank_offset = bank as usize * 0x4000;
                
                self.bank1.copy_from_slice(&self.program[bank_offset..=(bank_offset + 0x3fff)]);
            }
            0x4000..=0x5fff => {
                if self.banking_mode == AdvancedRomOrRamBanking {
                    let value = value & 0b0000_0011;
                    
                    if self.cartridge_ram_size_type == 3 {
                        self.ram_current_bank = value;
                    } else if false /* 1MiB ROM or larger */ {
                        // Use value for bits 4-5 of ROM bank number.
                    }
                }
            }
            0x6000..=0x7fff => {
                let value = value & 0b0000_0001;
                
                self.banking_mode = match value {
                    0 => Simple,
                    1 => AdvancedRomOrRamBanking,
                    _ => unreachable!()
                };
            }
            0x8000..=0x9fff => self.ppu.vram.mem_write(addr, value),
            0xa000..=0xbfff => {
                if !self.ram_enable {
                    return;
                }
                
                let bank = match self.banking_mode {
                    Simple => 0,
                    AdvancedRomOrRamBanking => self.ram_current_bank,
                };
                
                let addr = (addr - 0xa000) as usize;
                
                self.ram_banks[bank as usize][addr] = value;
            },
            0xc000..=0xdfff => self.wram[(addr - 0xc000) as usize] = value,
            0xe000..=0xfdff => self.wram[(addr - 0xe000) as usize] = value,
            0xfe00..=0xfe9f => self.ppu.vram.mem_write(addr, value),
            0xfea0..=0xfeff => {}, // panic!("not usable"),
            0xff00..=0xff7f => self.io_registers.mem_write(addr, value),
            0xff80..=0xfffe => self.hram[(addr - 0xff80) as usize] = value,
            0xffff => self.io_registers.mem_write(addr, value),
            _ => unreachable!()
        }
    }
}