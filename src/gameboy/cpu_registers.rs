use std::fmt;
use std::fmt::Formatter;
use bitflags::Flags;

bitflags! {
    #[derive(Copy, Clone)]
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

#[derive(Copy, Clone)]
pub struct CpuRegisters {
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
    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f.bits()])
    }
    pub fn set_af(&mut self, value: u16) {
        self.a = (value >> 8) as u8;
        self.f = CpuFlags::from_bits_truncate((value & 0xff) as u8);
    }

    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }
    pub fn set_bc(&mut self, value: u16) {
        [self.b, self.c] = value.to_be_bytes();
    }

    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }
    pub(crate) fn set_de(&mut self, value: u16) {
        [self.d, self.e] = value.to_be_bytes();
    }

    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }
    pub fn set_hl(&mut self, value: u16) {
        [self.h, self.l] = value.to_be_bytes();
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
