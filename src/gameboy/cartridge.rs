use super::{
    cartridge::BankingMode::{AdvancedRomOrRamBanking, Simple},
    Mem,
};

const OFFSET_TITLE: usize = 0x0134;
const OFFSET_CARTRIDGE_TYPE: usize = 0x0147;
const OFFSET_ROM_SIZE: usize = 0x0148;
const OFFSET_RAM_SIZE: usize = 0x0149;
const OFFSET_MASK_ROM_VERSION_NUMBER: usize = 0x014c;
const OFFSET_CHECKSUM: usize = 0x014d;

pub(crate) enum Mapper {
    None,
    MBC1,
}

#[derive(PartialEq, Clone, Copy)]
pub enum BankingMode {
    // 00 = Simple Banking Mode (default)
    //      0000–3FFF and A000–BFFF locked to bank 0 of ROM/RAM
    Simple,
    // 01 = RAM Banking Mode / Advanced ROM Banking Mode
    //      0000–3FFF and A000–BFFF can be bank-switched via the 4000–5FFF bank register
    AdvancedRomOrRamBanking,
}

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

fn verify_checksum(program: &Vec<u8>) -> bool {
    let mut checksum: u8 = 0;

    for i in OFFSET_TITLE..=OFFSET_MASK_ROM_VERSION_NUMBER {
        checksum = checksum.wrapping_sub(program[i]).wrapping_sub(1);
    }

    return checksum == program[OFFSET_CHECKSUM];
}

pub(crate) struct Cartridge {
    _program: Vec<u8>,
    mapper: Mapper,
    banking_mode: BankingMode,
    cartridge_rom_size_type: u8,
    rom_current_bank: u8,
    rom_secondary_bank_register: u8,
    rom_banks: Vec<[u8; 0x4000]>,
    cartridge_ram_size_type: u8,
    ram_enable: bool,
    ram_current_bank: u8,
    ram_banks: Vec<[u8; 0x2000]>,
}

impl Cartridge {
    pub fn load(program: Vec<u8>) -> Self {
        let _checksum = verify_checksum(&program);

        let cartridge_type = program[OFFSET_CARTRIDGE_TYPE];
        let mapper = match cartridge_type {
            0x00 | 0x08 | 0x09 => Mapper::None,
            0x01..=0x03 => Mapper::MBC1,
            0x05 | 0x06 => unimplemented!("MBC2"),
            0x0b..=0x0d => unimplemented!("MMM01"),
            0x0f..=0x13 => unimplemented!("MBC3"),
            0x19..=0x1e => unimplemented!("MBC5"),
            0x20 => unimplemented!("MBC6"),
            0x22 => unimplemented!("MBC7"),
            0xfc => unimplemented!("Pocket Camera"),
            0xfd => unimplemented!("Bandai TAMA5"),
            0xfe => unimplemented!("HuC3"),
            0xff => unimplemented!("HuC1"),
            _ => unreachable!()
        };

        let cartridge_rom_size_type = program[OFFSET_ROM_SIZE];
        let rom_size_bytes: usize = 32 * 1024 * (1 << cartridge_rom_size_type);

        let bank_count = rom_size_bytes / 0x4000;

        let mut rom_banks = Vec::with_capacity(bank_count);
        for i in 0..bank_count {
            let mut bank: [u8; 0x4000] = [0; 0x4000];
            bank.copy_from_slice(&program[(i * 0x4000)..=(i * 0x4000 + 0x3fff)]);

            rom_banks.push(bank);
        }

        let cartridge_ram_size_type = program[OFFSET_RAM_SIZE];

        let cartridge_ram_bytes_total = cartridge_ram_size_kib(cartridge_ram_size_type) * 1024;

        let mut ram_banks = Vec::with_capacity(cartridge_ram_bytes_total / 0x2000);
        while ram_banks.len() < ram_banks.capacity() {
            ram_banks.push([0; 0x2000]);
        }

        Self {
            _program: program,
            mapper,
            banking_mode: Simple,
            cartridge_rom_size_type,
            rom_current_bank: 1,
            rom_secondary_bank_register: 0,
            rom_banks,
            cartridge_ram_size_type,
            ram_enable: false,
            ram_current_bank: 0,
            ram_banks,
        }
    }

    fn mem_read_mbc_none(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3fff => self.rom_banks[0][addr as usize],
            0x4000..=0x7fff => self.rom_banks[1][(addr - 0x4000) as usize],
            0xa000..=0xbfff => {
                let addr = (addr - 0xa000) as usize;

                self.ram_banks[0][addr]
            }
            _ => unreachable!()
        }
    }

    fn mem_read_mbc1(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3fff => self.rom_banks[0][addr as usize],
            0x4000..=0x7fff => self.rom_banks[self.rom_current_bank as usize][(addr - 0x4000) as usize],
            0xa000..=0xbfff => {
                let addr = (addr - 0xa000) as usize;

                match (self.ram_enable, self.banking_mode) {
                    (false, _) => 0xff,
                    (_, Simple) => self.ram_banks[0][addr],
                    (_, AdvancedRomOrRamBanking) => self.ram_banks[self.ram_current_bank as usize][addr]
                }
            }
            _ => unreachable!()
        }
    }

    fn mem_write_mbc1(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1fff => {
                self.ram_enable = value & 0x0f == 0x0a;
            }
            0x2000..=0x3fff => {
                let bank_count_mask = match self.cartridge_rom_size_type {
                    0 => 0b0000_0001,
                    1 => 0b0000_0011,
                    2 => 0b0000_0111,
                    3 => 0b0000_1111,
                    _ => 0b0001_1111,
                };

                let mut bank = self.rom_secondary_bank_register << 5 | (value & bank_count_mask);

                let allow_bank0_mirroring = bank & 0b0001_0000 != 0;

                if bank == 0x00 {
                    bank = if allow_bank0_mirroring {
                        0x00
                    } else {
                        0x01
                    };
                }

                self.rom_current_bank = bank;
            }
            0x4000..=0x5fff => {
                if self.banking_mode == AdvancedRomOrRamBanking {
                    let value = value & 0b0000_0011;

                    if self.cartridge_ram_size_type == 3 {
                        self.ram_current_bank = value;
                    } else if self.cartridge_rom_size_type >= 5 {
                        // For 1MiB ROM or larger, use value for bits 4-5 of ROM bank number.
                        self.rom_secondary_bank_register = value;
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
            }
            _ => unreachable!()
        }
    }
}

impl Mem for Cartridge {
    fn mem_read(&self, addr: u16) -> u8 {
        return match self.mapper {
            Mapper::None => self.mem_read_mbc_none(addr),
            Mapper::MBC1 => self.mem_read_mbc1(addr),
        };
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        return match self.mapper {
            Mapper::None => {}
            Mapper::MBC1 => self.mem_write_mbc1(addr, value),
        };
    }
}