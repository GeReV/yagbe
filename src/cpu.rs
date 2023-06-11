﻿use std::{
    fs::File,
    fmt::Formatter,
    fmt
};
pub use bitflags::Flags;
use crate::Mem;
use crate::bus::{Bus, InterruptFlags};


bitflags! {
    pub struct CpuFlags : u8 {
        // In the documentation, flags are referred to in the order below.
        const ZERO = 1 << 7;
        const NEGATIVE = 1 << 6;
        const HALF_CARRY = 1 << 5;
        const CARRY = 1 << 4;
    }
}

impl Default for CpuFlags {
    fn default() -> Self {
        CpuFlags::from(CpuFlags::ZERO | CpuFlags::HALF_CARRY | CpuFlags::CARRY)
    }
}

fn invalid_instruction() {
    panic!("invalid instruction")
}

pub(crate) struct CpuRegisters {
    pub a: u8,
    pub f: CpuFlags,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}

impl CpuRegisters {
    fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f.bits()])
    }
    fn set_af(&mut self, value: u16) {
        self.a = (value >> 8) as u8;
        self.f = CpuFlags::from_bits_truncate((value & 0xff) as u8);
    }

    fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }
    fn set_bc(&mut self, value: u16) {
        self.b = (value >> 8) as u8;
        self.c = (value & 0xff) as u8;
    }

    fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }
    fn set_de(&mut self, value: u16) {
        self.d = (value >> 8) as u8;
        self.e = (value & 0xff) as u8;
    }

    fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }
    fn set_hl(&mut self, value: u16) {
        self.h = (value >> 8) as u8;
        self.l = (value & 0xff) as u8;
    }
}

impl fmt::Display for CpuRegisters {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let af = self.af();
        let bc = self.bc();
        let de = self.de();
        let hl = self.hl();

        let sp = self.sp;
        let pc = self.pc;

        write!(f, "BC={bc:04X} DE={de:04X} HL={hl:04X} AF={af:04X} SP={sp:04X} PC={pc:04X}")
    }
}

impl Default for CpuRegisters {
    // https://gbdev.io/pandocs/Power_Up_Sequence.html#cpu-registers
    fn default() -> Self {
        Self {
            a: 0x01,
            f: CpuFlags::default(),
            b: 0x00,
            c: 0x13,
            d: 0x00,
            e: 0xd8,
            h: 0x01,
            l: 0x4d,
            pc: 0x0100,
            sp: 0xfffe,
        }
    }
}

const OFFSET_CARTRIDGE_TYPE: usize = 0x0147;
const OFFSET_ROM_SIZE: usize = 0x0148;
const OFFSET_RAM_SIZE: usize = 0x0149;

pub struct Cpu {
    interrupts_master_enable: bool,
    registers: CpuRegisters,
    pub bus: Bus,
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            interrupts_master_enable: true,
            registers: Default::default(),
            bus: Bus::new(),
        }
    }

    pub fn load(&mut self, program: Vec<u8>) {
        let checksum = self.verify_checksum(&program);

        let cartridge_type = program[OFFSET_CARTRIDGE_TYPE];

        let rom_size_type = program[OFFSET_ROM_SIZE];
        let rom_size_bytes: usize = 32 * 1024 * (1 << rom_size_type);

        let ram_size_type = program[OFFSET_RAM_SIZE];

        self.bus.bank0.copy_from_slice(&program[0x0000..=0x3fff]);
        self.bus.bank1.copy_from_slice(&program[0x4000..=0x7fff]);
    }

    pub fn load_and_execute(&mut self, program: Vec<u8>) {
        self.load(program);

        loop {
            // self.execute();
        }
    }

    fn verify_checksum(&self, program: &Vec<u8>) -> bool {
        let mut checksum: u8 = 0;

        for i in 0x0134..=0x014c_usize {
            checksum = checksum.wrapping_sub(program[i]).wrapping_sub(1);
        }

        return checksum == program[0x014d];
    }

    pub fn run_to_frame(&mut self, log: &mut File) {
        loop {
            self.execute(log);

            if self.bus.ppu.tick(&mut self.bus.io_registers) {
                break;
            }
        }
    }

    fn execute(&mut self, _log: &mut File) -> usize {
        // let pc = self.registers.pc;

        if self.interrupt_service_routine() {
            return 5;
        }

        let instruction = self.read_u8();

        let m_cycles = 1usize;
        let t_cycles = m_cycles * 4;

        match instruction {
            0x00 => {}
            0x01 => {
                let value = self.read_u16();
                self.registers.set_bc(value);
            }
            0x02 => {
                self.bus.mem_write(self.registers.bc(), self.registers.a);
            }
            0x03 => {
                self.registers.set_bc(self.registers.bc().wrapping_add(1));
            }
            0x04 => {
                self.registers.b = self.inc_r8(self.registers.b);
            }
            0x05 => {
                self.registers.b = self.dec_r8(self.registers.b);
            }
            0x06 => {
                self.registers.b = self.read_u8();
            }
            0x07 => {
                self.registers.a = self.rlc(self.registers.a);
                self.registers.f.remove(CpuFlags::ZERO);
            }
            0x08 => {
                let addr = self.read_u16();
                self.bus.mem_write(addr, (self.registers.sp & 0xff) as u8);
                self.bus.mem_write(addr + 1, (self.registers.sp >> 8) as u8);
            }
            0x09 => {
                self.add_hl(self.registers.bc());
            }
            0x0a => {
                self.registers.a = self.bus.mem_read(self.registers.bc());
            }
            0x0b => {
                self.registers.set_bc(self.registers.bc().wrapping_sub(1));
            }
            0x0c => {
                self.registers.c = self.inc_r8(self.registers.c);
            }
            0x0d => {
                self.registers.c = self.dec_r8(self.registers.c);
            }
            0x0e => {
                self.registers.c = self.read_u8();
            }
            0x0f => {
                self.registers.a = self.rrc(self.registers.a);
                self.registers.f.remove(CpuFlags::ZERO);
            }
            0x10 => {
                let _ = self.read_u8();
                // TODO: Set standby?
            }
            0x11 => {
                let value = self.read_u16();
                self.registers.set_de(value);
            }
            0x12 => {
                self.bus.mem_write(self.registers.de(), self.registers.a);
            }
            0x13 => {
                let value = self.registers.de().wrapping_add(1);
                self.registers.set_de(value);
            }
            0x14 => {
                self.registers.d = self.inc_r8(self.registers.d);
            }
            0x15 => {
                self.registers.d = self.dec_r8(self.registers.d);
            }
            0x16 => {
                self.registers.d = self.read_u8();
            }
            0x17 => {
                self.registers.a = self.rl(self.registers.a);
                self.registers.f.remove(CpuFlags::ZERO);
            }
            0x18 => {
                let offset = self.read_i8();
                self.jr(offset);
            }
            0x19 => {
                self.add_hl(self.registers.de());
            }
            0x1a => {
                self.registers.a = self.bus.mem_read(self.registers.de());
            }
            0x1b => {
                self.registers.set_de(self.registers.de().wrapping_sub(1));
            }
            0x1c => {
                self.registers.e = self.inc_r8(self.registers.e);
            }
            0x1d => {
                self.registers.e = self.dec_r8(self.registers.e);
            }
            0x1e => {
                self.registers.e = self.read_u8();
            }
            0x1f => {
                self.registers.a = self.rr(self.registers.a);
                self.registers.f.remove(CpuFlags::ZERO);
            }
            0x20 => {
                let offset = self.read_i8();
                if !self.registers.f.contains(CpuFlags::ZERO) {
                    self.jr(offset);
                }
            }
            0x21 => {
                let value = self.read_u16();
                self.registers.set_hl(value);
            }
            0x22 => {
                self.bus.mem_write(self.registers.hl(), self.registers.a);
                self.inc_hl();
            }
            0x23 => {
                self.inc_hl();
            }
            0x24 => {
                self.registers.h = self.inc_r8(self.registers.h);
            }
            0x25 => {
                self.registers.h = self.dec_r8(self.registers.h);
            }
            0x26 => {
                self.registers.h = self.read_u8();
            }
            0x27 => self.daa(),
            0x28 => {
                let offset = self.read_i8();
                if self.registers.f.contains(CpuFlags::ZERO) {
                    self.jr(offset);
                }
            }
            0x29 => {
                self.add_hl(self.registers.hl());
            }
            0x2a => {
                self.registers.a = self.bus.mem_read(self.registers.hl());
                self.inc_hl();
            }
            0x2b => {
                self.dec_hl();
            }
            0x2c => {
                self.registers.l = self.inc_r8(self.registers.l);
            }
            0x2d => {
                self.registers.l = self.dec_r8(self.registers.l);
            }
            0x2e => {
                self.registers.l = self.read_u8();
            }
            0x2f => {
                self.registers.a = !self.registers.a;
                self.registers.f.insert(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY);
            }
            0x30 => {
                let offset = self.read_i8();
                if !self.registers.f.contains(CpuFlags::CARRY) {
                    self.jr(offset);
                }
            }
            0x31 => {
                self.registers.sp = self.read_u16();
            }
            0x32 => {
                self.bus.mem_write(self.registers.hl(), self.registers.a);
                self.dec_hl();
            }
            0x33 => {
                self.registers.sp = self.registers.sp.wrapping_add(1);
            }
            0x34 => {
                let addr = self.registers.hl();
                let value = self.bus.mem_read(addr);
                let result = value.wrapping_add(1);

                self.registers.f.set(CpuFlags::ZERO, result == 0);
                self.registers.f.remove(CpuFlags::NEGATIVE);
                self.registers.f.set(CpuFlags::HALF_CARRY, value == 0x0f);

                self.bus.mem_write(addr, result);
            }
            0x35 => {
                let addr = self.registers.hl();
                let value = self.bus.mem_read(addr);
                let result = value.wrapping_sub(1);

                self.registers.f.set(CpuFlags::ZERO, result == 0);
                self.registers.f.insert(CpuFlags::NEGATIVE);
                self.registers.f.set(CpuFlags::HALF_CARRY, value == 0);

                self.bus.mem_write(addr, result);
            }
            0x36 => {
                let value = self.read_u8();
                self.bus.mem_write(self.registers.hl(), value);
            }
            0x37 => {
                self.registers.f.insert(CpuFlags::CARRY);
                self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY);
            }
            0x38 => {
                let offset = self.read_i8();
                if self.registers.f.contains(CpuFlags::CARRY) {
                    self.jr(offset);
                }
            }
            0x39 => {
                self.add_hl(self.registers.sp);
            }
            0x3a => {
                self.registers.a = self.bus.mem_read(self.registers.hl());
                self.dec_hl();
            }
            0x3b => {
                self.registers.sp = self.registers.sp.wrapping_sub(1);
            }
            0x3c => {
                self.registers.a = self.inc_r8(self.registers.a);
            }
            0x3d => {
                self.registers.a = self.dec_r8(self.registers.a);
            }
            0x3e => {
                self.registers.a = self.read_u8();
            }
            0x3f => {
                self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY);
                self.registers.f.toggle(CpuFlags::CARRY);
            }
            0x40 => self.registers.b = self.registers.b,
            0x41 => self.registers.b = self.registers.c,
            0x42 => self.registers.b = self.registers.d,
            0x43 => self.registers.b = self.registers.e,
            0x44 => self.registers.b = self.registers.h,
            0x45 => self.registers.b = self.registers.l,
            0x46 => self.registers.b = self.bus.mem_read(self.registers.hl()),
            0x47 => self.registers.b = self.registers.a,
            0x48 => self.registers.c = self.registers.b,
            0x49 => self.registers.c = self.registers.c,
            0x4a => self.registers.c = self.registers.d,
            0x4b => self.registers.c = self.registers.e,
            0x4c => self.registers.c = self.registers.h,
            0x4d => self.registers.c = self.registers.l,
            0x4e => self.registers.c = self.bus.mem_read(self.registers.hl()),
            0x4f => self.registers.c = self.registers.a,
            0x50 => self.registers.d = self.registers.b,
            0x51 => self.registers.d = self.registers.c,
            0x52 => self.registers.d = self.registers.d,
            0x53 => self.registers.d = self.registers.e,
            0x54 => self.registers.d = self.registers.h,
            0x55 => self.registers.d = self.registers.l,
            0x56 => self.registers.d = self.bus.mem_read(self.registers.hl()),
            0x57 => self.registers.d = self.registers.a,
            0x58 => self.registers.e = self.registers.b,
            0x59 => self.registers.e = self.registers.c,
            0x5a => self.registers.e = self.registers.d,
            0x5b => self.registers.e = self.registers.e,
            0x5c => self.registers.e = self.registers.h,
            0x5d => self.registers.e = self.registers.l,
            0x5e => self.registers.e = self.bus.mem_read(self.registers.hl()),
            0x5f => self.registers.e = self.registers.a,
            0x60 => self.registers.h = self.registers.b,
            0x61 => self.registers.h = self.registers.c,
            0x62 => self.registers.h = self.registers.d,
            0x63 => self.registers.h = self.registers.e,
            0x64 => self.registers.h = self.registers.h,
            0x65 => self.registers.h = self.registers.l,
            0x66 => self.registers.h = self.bus.mem_read(self.registers.hl()),
            0x67 => self.registers.h = self.registers.a,
            0x68 => self.registers.l = self.registers.b,
            0x69 => self.registers.l = self.registers.c,
            0x6a => self.registers.l = self.registers.d,
            0x6b => self.registers.l = self.registers.e,
            0x6c => self.registers.l = self.registers.h,
            0x6d => self.registers.l = self.registers.l,
            0x6e => self.registers.l = self.bus.mem_read(self.registers.hl()),
            0x6f => self.registers.l = self.registers.a,
            0x70 => self.bus.mem_write(self.registers.hl(), self.registers.b),
            0x71 => self.bus.mem_write(self.registers.hl(), self.registers.c),
            0x72 => self.bus.mem_write(self.registers.hl(), self.registers.d),
            0x73 => self.bus.mem_write(self.registers.hl(), self.registers.e),
            0x74 => self.bus.mem_write(self.registers.hl(), self.registers.h),
            0x75 => self.bus.mem_write(self.registers.hl(), self.registers.l),
            0x76 => { /* HALT */ }
            0x77 => self.bus.mem_write(self.registers.hl(), self.registers.a),
            0x78 => self.registers.a = self.registers.b,
            0x79 => self.registers.a = self.registers.c,
            0x7a => self.registers.a = self.registers.d,
            0x7b => self.registers.a = self.registers.e,
            0x7c => self.registers.a = self.registers.h,
            0x7d => self.registers.a = self.registers.l,
            0x7e => self.registers.a = self.bus.mem_read(self.registers.hl()),
            0x7f => self.registers.a = self.registers.a,
            0x80 => self.registers.a = self.add(self.registers.a, self.registers.b),
            0x81 => self.registers.a = self.add(self.registers.a, self.registers.c),
            0x82 => self.registers.a = self.add(self.registers.a, self.registers.d),
            0x83 => self.registers.a = self.add(self.registers.a, self.registers.e),
            0x84 => self.registers.a = self.add(self.registers.a, self.registers.h),
            0x85 => self.registers.a = self.add(self.registers.a, self.registers.l),
            0x86 => self.registers.a = self.add(self.registers.a, self.bus.mem_read(self.registers.hl())),
            0x87 => self.registers.a = self.add(self.registers.a, self.registers.a),
            0x88 => self.registers.a = self.adc(self.registers.a, self.registers.b),
            0x89 => self.registers.a = self.adc(self.registers.a, self.registers.c),
            0x8a => self.registers.a = self.adc(self.registers.a, self.registers.d),
            0x8b => self.registers.a = self.adc(self.registers.a, self.registers.e),
            0x8c => self.registers.a = self.adc(self.registers.a, self.registers.h),
            0x8d => self.registers.a = self.adc(self.registers.a, self.registers.l),
            0x8e => self.registers.a = self.adc(self.registers.a, self.bus.mem_read(self.registers.hl())),
            0x8f => self.registers.a = self.adc(self.registers.a, self.registers.a),
            0x90 => self.registers.a = self.sub(self.registers.a, self.registers.b),
            0x91 => self.registers.a = self.sub(self.registers.a, self.registers.c),
            0x92 => self.registers.a = self.sub(self.registers.a, self.registers.d),
            0x93 => self.registers.a = self.sub(self.registers.a, self.registers.e),
            0x94 => self.registers.a = self.sub(self.registers.a, self.registers.h),
            0x95 => self.registers.a = self.sub(self.registers.a, self.registers.l),
            0x96 => self.registers.a = self.sub(self.registers.a, self.bus.mem_read(self.registers.hl())),
            0x97 => self.registers.a = self.sub(self.registers.a, self.registers.a),
            0x98 => self.registers.a = self.sbc(self.registers.a, self.registers.b),
            0x99 => self.registers.a = self.sbc(self.registers.a, self.registers.c),
            0x9a => self.registers.a = self.sbc(self.registers.a, self.registers.d),
            0x9b => self.registers.a = self.sbc(self.registers.a, self.registers.e),
            0x9c => self.registers.a = self.sbc(self.registers.a, self.registers.h),
            0x9d => self.registers.a = self.sbc(self.registers.a, self.registers.l),
            0x9e => self.registers.a = self.sbc(self.registers.a, self.bus.mem_read(self.registers.hl())),
            0x9f => self.registers.a = self.sbc(self.registers.a, self.registers.a),
            0xa0 => self.registers.a = self.and(self.registers.a, self.registers.b),
            0xa1 => self.registers.a = self.and(self.registers.a, self.registers.c),
            0xa2 => self.registers.a = self.and(self.registers.a, self.registers.d),
            0xa3 => self.registers.a = self.and(self.registers.a, self.registers.e),
            0xa4 => self.registers.a = self.and(self.registers.a, self.registers.h),
            0xa5 => self.registers.a = self.and(self.registers.a, self.registers.l),
            0xa6 => self.registers.a = self.and(self.registers.a, self.bus.mem_read(self.registers.hl())),
            0xa7 => self.registers.a = self.and(self.registers.a, self.registers.a),
            0xa8 => self.registers.a = self.xor(self.registers.a, self.registers.b),
            0xa9 => self.registers.a = self.xor(self.registers.a, self.registers.c),
            0xaa => self.registers.a = self.xor(self.registers.a, self.registers.d),
            0xab => self.registers.a = self.xor(self.registers.a, self.registers.e),
            0xac => self.registers.a = self.xor(self.registers.a, self.registers.h),
            0xad => self.registers.a = self.xor(self.registers.a, self.registers.l),
            0xae => self.registers.a = self.xor(self.registers.a, self.bus.mem_read(self.registers.hl())),
            0xaf => self.registers.a = self.xor(self.registers.a, self.registers.a),
            0xb0 => self.registers.a = self.or(self.registers.a, self.registers.b),
            0xb1 => self.registers.a = self.or(self.registers.a, self.registers.c),
            0xb2 => self.registers.a = self.or(self.registers.a, self.registers.d),
            0xb3 => self.registers.a = self.or(self.registers.a, self.registers.e),
            0xb4 => self.registers.a = self.or(self.registers.a, self.registers.h),
            0xb5 => self.registers.a = self.or(self.registers.a, self.registers.l),
            0xb6 => self.registers.a = self.or(self.registers.a, self.bus.mem_read(self.registers.hl())),
            0xb7 => self.registers.a = self.or(self.registers.a, self.registers.a),
            0xb8 => self.cp(self.registers.a, self.registers.b),
            0xb9 => self.cp(self.registers.a, self.registers.c),
            0xba => self.cp(self.registers.a, self.registers.d),
            0xbb => self.cp(self.registers.a, self.registers.e),
            0xbc => self.cp(self.registers.a, self.registers.h),
            0xbd => self.cp(self.registers.a, self.registers.l),
            0xbe => self.cp(self.registers.a, self.bus.mem_read(self.registers.hl())),
            0xbf => self.cp(self.registers.a, self.registers.a),
            0xc0 => {
                if !self.registers.f.contains(CpuFlags::ZERO) {
                    self.ret();
                }
            }
            0xc1 => {
                let value = self.pop();
                self.registers.set_bc(value)
            }
            0xc2 => {
                let addr = self.read_u16();
                if !self.registers.f.contains(CpuFlags::ZERO) {
                    self.registers.pc = addr;
                }
            }
            0xc3 => self.registers.pc = self.read_u16(),
            0xc4 => {
                let addr = self.read_u16();
                if !self.registers.f.contains(CpuFlags::ZERO) {
                    self.call(addr);
                }
            }
            0xc5 => self.push(self.registers.bc()),
            0xc6 => {
                let value = self.read_u8();
                self.registers.a = self.add(self.registers.a, value);
            }
            0xc7 => self.call(0x0000),
            0xc8 => {
                if self.registers.f.contains(CpuFlags::ZERO) {
                    self.ret()
                }
            }
            0xc9 => self.ret(),
            0xca => {
                let addr = self.read_u16();
                if self.registers.f.contains(CpuFlags::ZERO) {
                    self.registers.pc = addr;
                }
            }
            0xcb => {
                let value = self.read_u8();
                self.cb(value);
            }
            0xcc => {
                let addr = self.read_u16();
                if self.registers.f.contains(CpuFlags::ZERO) {
                    self.call(addr);
                }
            }
            0xcd => {
                let addr = self.read_u16();
                self.call(addr);
            }
            0xce => {
                let value = self.read_u8();
                self.registers.a = self.adc(self.registers.a, value)
            }
            0xcf => self.call(0x0008),
            0xd0 => {
                if !self.registers.f.contains(CpuFlags::CARRY) {
                    self.ret();
                }
            }
            0xd1 => {
                let value = self.pop();
                self.registers.set_de(value);
            }
            0xd2 => {
                let addr = self.read_u16();
                if !self.registers.f.contains(CpuFlags::CARRY) {
                    self.registers.pc = addr;
                }
            }
            0xd3 => invalid_instruction(),
            0xd4 => {
                let addr = self.read_u16();
                if !self.registers.f.contains(CpuFlags::CARRY) {
                    self.call(addr);
                }
            }
            0xd5 => self.push(self.registers.de()),
            0xd6 => {
                let value = self.read_u8();
                self.registers.a = self.sub(self.registers.a, value);
            }
            0xd7 => self.call(0x0010),
            0xd8 => {
                if self.registers.f.contains(CpuFlags::CARRY) {
                    self.ret()
                }
            }
            0xd9 => self.reti(),
            0xda => {
                let addr = self.read_u16();
                if self.registers.f.contains(CpuFlags::CARRY) {
                    self.registers.pc = addr;
                }
            }
            0xdb => invalid_instruction(),
            0xdc => {
                let addr = self.read_u16();
                if self.registers.f.contains(CpuFlags::CARRY) {
                    self.call(addr);
                }
            }
            0xdd => invalid_instruction(),
            0xde => {
                let value = self.read_u8();
                self.registers.a = self.sbc(self.registers.a, value)
            }
            0xdf => self.call(0x0018),
            0xe0 => {
                let value = self.read_u8();
                self.bus.mem_write(0xff00 + value as u16, self.registers.a);
            }
            0xe1 => {
                let value = self.pop();
                self.registers.set_hl(value);
            }
            0xe2 => {
                let value = self.read_u8();
                self.bus.mem_write(0xff00 + value as u16, self.registers.a);
            }
            0xe3 => invalid_instruction(),
            0xe4 => invalid_instruction(),
            0xe5 => self.push(self.registers.hl()),
            0xe6 => {
                let value = self.read_u8();
                self.registers.a = self.and(self.registers.a, value);
            }
            0xe7 => self.call(0x0020),
            0xe8 => {
                let value = self.read_i8() as u16;
                
                // NOTE(grozki): I initially thought this u16::wrapping_add_signed() would work, but it doesn't work with the carry math below.
                let result = self.registers.sp.wrapping_add(value);
                                
                self.registers.f.remove(CpuFlags::ZERO | CpuFlags::NEGATIVE);
                self.registers.f.set(CpuFlags::HALF_CARRY, ((self.registers.sp & 0x0f) + (value & 0x0f)) & 0x10 != 0);
                self.registers.f.set(CpuFlags::CARRY, ((self.registers.sp & 0xff) + (value & 0xff)) & 0x100 != 0);

                self.registers.sp = result;
            }
            0xe9 => self.registers.pc = self.registers.hl(),
            0xea => {
                let addr = self.read_u16();
                self.bus.mem_write(addr, self.registers.a);
            }
            0xeb => invalid_instruction(),
            0xec => invalid_instruction(),
            0xed => invalid_instruction(),
            0xee => {
                let value = self.read_u8();
                self.registers.a = self.xor(self.registers.a, value);
            }
            0xef => self.call(0x0028),
            0xf0 => {
                // TODO: Check if address is 0xff00..=0xffff?
                let offset = self.read_u8();
                self.registers.a = self.bus.mem_read(0xff00 + offset as u16);
            }
            0xf1 => {
                let value = self.pop();
                self.registers.set_af(value);
            }
            0xf2 => {
                let value = self.read_u8();
                self.registers.a = self.bus.mem_read(0xff00 + value as u16);
            }
            0xf3 => self.interrupts_master_enable = false,
            0xf4 => invalid_instruction(),
            0xf5 => {
                let af = self.registers.af();
                self.push(af);
            }
            0xf6 => {
                let value = self.read_u8();
                self.registers.a = self.or(self.registers.a, value);
            }
            0xf7 => self.call(0x0030),
            0xf8 => {
                let value = self.read_i8() as u16;
                let result = self.registers.sp.wrapping_add(value);

                self.registers.f.remove(CpuFlags::ZERO | CpuFlags::NEGATIVE);
                self.registers.f.set(CpuFlags::HALF_CARRY, ((self.registers.sp & 0x0f) + (value & 0x0f)) & 0x10 != 0);
                self.registers.f.set(CpuFlags::CARRY, ((self.registers.sp & 0xff) + (value & 0xff)) & 0x100 != 0);
                
                self.registers.set_hl(result);
            }
            0xf9 => self.registers.sp = self.registers.hl(),
            0xfa => {
                let addr = self.read_u16();
                self.registers.a = self.bus.mem_read(addr);
            }
            0xfb => self.interrupts_master_enable = true,
            0xfc => invalid_instruction(),
            0xfd => invalid_instruction(),
            0xfe => {
                let value = self.read_u8();
                self.cp(self.registers.a, value);
            }
            0xff => self.call(0x0038),
            _ => panic!("unknown instruction {instruction}")
        }

        // writeln!(log, "{}", self.registers).unwrap();
        // println!("{}", self.registers);

        return t_cycles;
    }

    fn cb(&mut self, value: u8) {
        let register_value = match value & 0x7 {
            0x0 => self.registers.b,
            0x1 => self.registers.c,
            0x2 => self.registers.d,
            0x3 => self.registers.e,
            0x4 => self.registers.h,
            0x5 => self.registers.l,
            0x6 => self.bus.mem_read(self.registers.hl()),
            0x7 => self.registers.a,
            _ => unreachable!()
        };

        let result = match value >> 3 {
            0x00 => Some(self.rlc(register_value)),
            0x01 => Some(self.rrc(register_value)),
            0x02 => Some(self.rl(register_value)),
            0x03 => Some(self.rr(register_value)),
            0x04 => Some(self.sla(register_value)),
            0x05 => Some(self.sra(register_value)),
            0x06 => Some(self.swap(register_value)),
            0x07 => Some(self.srl(register_value)),
            0x08..=0x0f => {
                self.bit((value >> 3) - 0x08, register_value);
                None
            }
            0x10..=0x17 => Some(self.res((value >> 3) - 0x08, register_value)),
            0x18..=0x1f => Some(self.set((value >> 3) - 0x08, register_value)),
            _ => unreachable!()
        };

        if let Some(result) = result {
            match value & 0x7 {
                0x0 => self.registers.b = result,
                0x1 => self.registers.c = result,
                0x2 => self.registers.d = result,
                0x3 => self.registers.e = result,
                0x4 => self.registers.h = result,
                0x5 => self.registers.l = result,
                0x6 => self.bus.mem_write(self.registers.hl(), result),
                0x7 => self.registers.a = result,
                _ => unreachable!()
            };
        }
    }

    fn add(&mut self, register_value: u8, value: u8) -> u8 {
        let (result, carry) = register_value.overflowing_add(value);

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE);
        self.registers.f.set(CpuFlags::HALF_CARRY, ((register_value & 0x0f) + (value & 0x0f)) & 0x10 != 0);
        self.registers.f.set(CpuFlags::CARRY, carry);

        return result;
    }

    fn sub(&mut self, register_value: u8, value: u8) -> u8 {
        let (result, carry) = register_value.overflowing_sub(value);

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.insert(CpuFlags::NEGATIVE);
        self.registers.f.set(CpuFlags::HALF_CARRY, (register_value & 0x0f).wrapping_sub(value & 0x0f) & 0x10 != 0);
        self.registers.f.set(CpuFlags::CARRY, carry);

        return result;
    }

    fn adc(&mut self, register_value: u8, value: u8) -> u8 {
        let c = if self.registers.f.contains(CpuFlags::CARRY) { 1 } else { 0 };
        let (result, carry1) = value.overflowing_add(c);
        let (result, carry2) = register_value.overflowing_add(result);

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE);
        self.registers.f.set(CpuFlags::HALF_CARRY, ((register_value & 0x0f) + (value & 0x0f) + c) & 0x10 != 0);
        self.registers.f.set(CpuFlags::CARRY, carry1 || carry2);

        return result;
    }

    fn sbc(&mut self, register_value: u8, value: u8) -> u8 {
        let c = if self.registers.f.contains(CpuFlags::CARRY) { 1 } else { 0 };
        let (result, carry1) = register_value.overflowing_sub(value);
        let (result, carry2) = result.overflowing_sub(c);

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.insert(CpuFlags::NEGATIVE);
        self.registers.f.set(CpuFlags::HALF_CARRY, (register_value & 0x0f).wrapping_sub(value & 0x0f).wrapping_sub(c) & 0x10 != 0);
        self.registers.f.set(CpuFlags::CARRY, carry1 || carry2);

        return result;
    }

    fn and(&mut self, register_value: u8, value: u8) -> u8 {
        let result = register_value & value;

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::CARRY);
        self.registers.f.insert(CpuFlags::HALF_CARRY);

        return result;
    }

    fn xor(&mut self, register_value: u8, value: u8) -> u8 {
        let result = register_value ^ value;

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY | CpuFlags::CARRY);

        return result;
    }

    fn or(&mut self, register_value: u8, value: u8) -> u8 {
        let result = register_value | value;

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY | CpuFlags::CARRY);

        return result;
    }

    fn cp(&mut self, register_value: u8, value: u8) {
        let (result, carry) = register_value.overflowing_sub(value);

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.insert(CpuFlags::NEGATIVE);
        self.registers.f.set(CpuFlags::HALF_CARRY, (register_value & 0x0f).wrapping_sub(value & 0x0f) & 0x10 != 0);
        self.registers.f.set(CpuFlags::CARRY, carry);
    }

    fn daa(&mut self) {
        let mut result = self.registers.a;
        let mut correction = 0;

        if self.registers.f.contains(CpuFlags::HALF_CARRY) || (!self.registers.f.contains(CpuFlags::NEGATIVE) && (self.registers.a & 0x0f) > 0x09) {
            correction |= 0x06;
        }

        if self.registers.f.contains(CpuFlags::CARRY) || (!self.registers.f.contains(CpuFlags::NEGATIVE) && self.registers.a > 0x99) {
            correction |= 0x60;

            self.registers.f.insert(CpuFlags::CARRY);
        }

        result = result.wrapping_add_signed(if self.registers.f.contains(CpuFlags::NEGATIVE) { -correction } else { correction });

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::HALF_CARRY);

        self.registers.a = result;
    }

    fn sla(&mut self, register_value: u8) -> u8 {
        let carry = register_value >> 7 == 1;
        let result = register_value << 1;

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY);
        self.registers.f.set(CpuFlags::CARRY, carry);

        return result;
    }

    fn sra(&mut self, register_value: u8) -> u8 {
        let carry = register_value & 1 == 1;
        let result = register_value >> 1;

        let result = (register_value & 0b1000_0000) | result & 0b0111_1111;

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY);
        self.registers.f.set(CpuFlags::CARRY, carry);

        return result;
    }

    fn srl(&mut self, register_value: u8) -> u8 {
        let carry = register_value & 1 == 1;
        let result = register_value >> 1;

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY);
        self.registers.f.set(CpuFlags::CARRY, carry);

        return result;
    }

    fn bit(&mut self, bit: u8, register_value: u8) {
        let mask = 1u8.wrapping_shl(bit as u32);

        self.registers.f.set(CpuFlags::ZERO, register_value & mask == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE);
        self.registers.f.insert(CpuFlags::HALF_CARRY);
    }

    fn res(&self, bit: u8, register_value: u8) -> u8 {
        let mask = 1u8.wrapping_shl(bit as u32);

        return register_value & !mask;
    }

    fn set(&self, bit: u8, register_value: u8) -> u8 {
        let mask = 1u8.wrapping_shl(bit as u32);

        return register_value | mask;
    }

    fn swap(&mut self, register_value: u8) -> u8 {
        let result = (register_value & 0x0f) << 4 | (register_value >> 4);

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY | CpuFlags::CARRY);

        return result;
    }

    fn inc_hl(&mut self) {
        self.registers.set_hl(self.registers.hl().wrapping_add(1));
    }

    fn dec_hl(&mut self) {
        self.registers.set_hl(self.registers.hl().wrapping_sub(1));
    }

    fn add_hl(&mut self, register_value: u16) {
        let (result, carry) = self.registers.hl().overflowing_add(register_value);

        self.registers.f.remove(CpuFlags::NEGATIVE);
        self.registers.f.set(CpuFlags::HALF_CARRY, ((self.registers.hl() & 0x0fff) + (register_value & 0xfff)) & 0x1000 != 0);
        self.registers.f.set(CpuFlags::CARRY, carry);

        self.registers.set_hl(result);
    }

    fn inc_r8(&mut self, register_value: u8) -> u8 {
        let value = register_value.wrapping_add(1);

        self.registers.f.set(CpuFlags::ZERO, value == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE);
        self.registers.f.set(CpuFlags::HALF_CARRY, register_value & 0x0f == 0x0f);

        return value;
    }

    fn dec_r8(&mut self, register_value: u8) -> u8 {
        let value = register_value.wrapping_sub(1);

        self.registers.f.set(CpuFlags::ZERO, value == 0);
        self.registers.f.insert(CpuFlags::NEGATIVE);
        self.registers.f.set(CpuFlags::HALF_CARRY, register_value & 0x0f == 0);

        return value;
    }

    fn rlc(&mut self, register_value: u8) -> u8 {
        let result = register_value.rotate_left(1);

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY);
        self.registers.f.set(CpuFlags::CARRY, register_value >> 7 == 1);

        return result;
    }

    fn rl(&mut self, register_value: u8) -> u8 {
        let carry: u8 = if self.registers.f.contains(CpuFlags::CARRY) { 1 } else { 0 };
        let did_carry = register_value >> 7 == 1;
        let result = register_value << 1;

        let result = result & 0b11111110 | carry;

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY);
        self.registers.f.set(CpuFlags::CARRY, did_carry);

        return result;
    }

    fn rrc(&mut self, register_value: u8) -> u8 {
        let result = register_value.rotate_right(1);

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY);
        self.registers.f.set(CpuFlags::CARRY, register_value & 1 == 1);

        return result;
    }

    fn rr(&mut self, register_value: u8) -> u8 {
        let carry: u8 = if self.registers.f.contains(CpuFlags::CARRY) { 1 } else { 0 };
        let did_carry = register_value & 1 == 1;
        let result = register_value >> 1;

        let result = result & 0b01111111 | carry << 7;

        self.registers.f.set(CpuFlags::ZERO, result == 0);
        self.registers.f.remove(CpuFlags::NEGATIVE | CpuFlags::HALF_CARRY);
        self.registers.f.set(CpuFlags::CARRY, did_carry);

        return result;
    }

    fn jr(&mut self, offset: i8) {
        self.registers.pc = self.registers.pc.wrapping_add_signed(offset as i16);
    }

    fn ret(&mut self) {
        self.registers.pc = self.pop();
    }

    fn reti(&mut self) {
        self.ret();

        self.interrupts_master_enable = true;
    }

    fn pop(&mut self) -> u16 {
        let lo = self.bus.mem_read(self.registers.sp);

        self.registers.sp = self.registers.sp.wrapping_add(1);

        let hi = self.bus.mem_read(self.registers.sp);

        self.registers.sp = self.registers.sp.wrapping_add(1);

        return u16::from_be_bytes([hi, lo]);
    }

    fn push(&mut self, register_value: u16) {
        self.registers.sp = self.registers.sp.wrapping_sub(2);

        self.bus.mem_write(self.registers.sp + 0, (register_value & 0xff) as u8);
        self.bus.mem_write(self.registers.sp + 1, (register_value >> 8) as u8);
    }

    fn call(&mut self, addr: u16) {
        self.registers.sp = self.registers.sp.wrapping_sub(2);

        self.bus.mem_write(self.registers.sp, (self.registers.pc & 0xff) as u8);
        self.bus.mem_write(self.registers.sp.wrapping_add(1), (self.registers.pc >> 8 & 0xff) as u8);

        self.registers.pc = addr;
    }

    fn interrupt_service_routine(&mut self) -> bool {
        if !self.interrupts_master_enable {
            return false;
        }

        self.interrupts_master_enable = false;

        let mut handled = false;

        for flag in InterruptFlags::all().iter() {
            if self.bus.io_registers.interrupt_enable.contains(flag) && self.bus.io_registers.interrupt_flag.contains(flag) {
                self.bus.io_registers.interrupt_flag.remove(flag);

                let handler_addr = match flag {
                    InterruptFlags::VBLANK => 0x0040,
                    InterruptFlags::LCD_STAT => 0x0048,
                    InterruptFlags::TIMER => 0x0050,
                    InterruptFlags::SERIAL => 0x0058,
                    InterruptFlags::JOYPAD => 0x0060,
                    _ => unreachable!()
                };

                self.call(handler_addr);

                handled = true;

                break;
            }
        }

        // TODO: Necessary, or rely on RETI instruction?
        self.interrupts_master_enable = true;

        return handled;
    }

    fn read_u8(&mut self) -> u8 {
        let addr = self.registers.pc;

        self.registers.pc = self.registers.pc.wrapping_add(1);

        return self.bus.mem_read(addr);
    }

    fn read_i8(&mut self) -> i8 {
        return self.read_u8() as i8;
    }

    fn read_u16(&mut self) -> u16 {
        return u16::from_le_bytes([self.read_u8(), self.read_u8()]);
    }
}