﻿use bitflags::Flags;
use crate::Mem;

bitflags! {
    #[derive(Default, Copy, Clone, PartialEq, Eq, Debug)]
    pub struct InterruptFlags : u8 {
        const VBLANK = 1 << 0;
        const LCD_STAT = 1 << 1;
        const TIMER = 1 << 2;
        const SERIAL = 1 << 3;
        const JOYPAD = 1 << 4;
    }
}

#[derive(Default)]
pub struct IoRegisters {
    pub joyp: u8,
    pub sb: u8,
    pub sc: u8,
    pub div: u8,
    pub cpu_clock: u16,
    pub tima: u8,
    pub clock_accumulator: usize,
    pub tma: u8,
    pub tac: u8,
    pub interrupt_flag: InterruptFlags,
    pub nr10: u8,
    pub nr11: u8,
    pub nr12: u8,
    pub nr13: u8,
    pub nr14: u8,
    pub nr21: u8,
    pub nr22: u8,
    pub nr23: u8,
    pub nr24: u8,
    pub nr30: u8,
    pub nr31: u8,
    pub nr32: u8,
    pub nr33: u8,
    pub nr34: u8,
    pub nr41: u8,
    pub nr42: u8,
    pub nr43: u8,
    pub nr44: u8,
    pub nr50: u8,
    pub nr51: u8,
    pub nr52: u8,
    pub wave_ram: [u8; 0x10],
    pub lcdc: u8,
    pub stat: u8,
    pub scy: u8,
    pub scx: u8,
    pub ly: u8,
    pub lyc: u8,
    pub dma: u8,
    pub bgp: u8,
    pub obp0: u8,
    pub obp1: u8,
    pub wy: u8,
    pub wx: u8,
    pub key1: u8,
    pub vbk: u8,
    pub hdma1: u8,
    pub hdma2: u8,
    pub hdma3: u8,
    pub hdma4: u8,
    pub hdma5: u8,
    pub rp: u8,
    pub bcps: u8,
    pub bcpd: u8,
    pub ocps: u8,
    pub ocpd: u8,
    pub opri: u8,
    pub svbk: u8,
    pub interrupt_enable: InterruptFlags,
}

impl Mem for IoRegisters {
    fn mem_read(&self, addr: u16) -> u8 {
        return match addr {
            0xff00 => self.joyp,
            0xff01 => self.sb,
            0xff02 => self.sc,
            0xff04 => self.div,
            0xff05 => self.tima,
            0xff06 => self.tma,
            0xff07 => self.tac,
            0xff0f => self.interrupt_flag.bits(),
            0xff10 => self.nr10,
            0xff11 => self.nr11,
            0xff12 => self.nr12,
            0xff13 => panic!("cannot read nr13 register"),
            0xff14 => self.nr14,
            0xff16 => self.nr21,
            0xff17 => self.nr22,
            0xff18 => panic!("cannot read nr23 register"),
            0xff19 => self.nr24,
            0xff1a => self.nr30,
            0xff1b => panic!("cannot read nr31 register"),
            0xff1c => self.nr32,
            0xff1d => panic!("cannot read nr33 register"),
            0xff1e => self.nr34,
            0xff20 => panic!("cannot read nr41 register"),
            0xff21 => self.nr42,
            0xff22 => self.nr43,
            0xff23 => self.nr44,
            0xff24 => self.nr50,
            0xff25 => self.nr51,
            0xff26 => self.nr52,
            0xff30..=0xff3f => self.wave_ram[(addr - 0xff30) as usize],
            0xff40 => self.lcdc,
            0xff41 => self.stat,
            0xff42 => self.scy,
            0xff43 => self.scx,
            0xff44 => self.ly,
            0xff45 => self.lyc,
            0xff46 => self.dma,
            0xff47 => self.bgp,
            0xff48 => self.obp0,
            0xff49 => self.obp1,
            0xff4a => self.wy,
            0xff4b => self.wx,
            0xff4d => self.key1,
            0xff4f => self.vbk,
            0xff51..=0xff55 => panic!("cannot read hdma registers"),
            0xff56 => self.rp,
            0xff68 => self.bcps,
            0xff69 => self.bcpd,
            0xff6a => self.ocps,
            0xff6b => self.ocpd,
            0xff6c => self.opri,
            0xff70 => self.svbk,
            0xff76 => panic!("cgb only"),
            0xff77 => panic!("cgb only"),
            0xffff => self.interrupt_enable.bits(),
            _ => 0xff, //panic!("invalid IO register address")
        };
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        return match addr {
            0xff00 => self.joyp = value & 0b0011_0000,
            0xff01 => self.sb = value,
            0xff02 => self.sc = value,
            0xff04 => {
                self.div = 0;
                self.cpu_clock = 0;
            }
            0xff05 => self.tima = value,
            0xff06 => self.tma = value,
            0xff07 => {
                if (self.tac & 0b0000_0011) != (value & 0b0000_0011) {
                    self.tima = self.tma;
                    self.clock_accumulator = 0;
                }
                
                self.tac = 0xf8 | value;
            }
            0xff0f => self.interrupt_flag = InterruptFlags::from_bits_truncate(value),
            0xff10 => self.nr10 = value,
            0xff11 => self.nr11 = value, // TODO: Mixed?
            0xff12 => self.nr12 = value,
            0xff13 => self.nr13 = value,
            0xff14 => self.nr14 = value, // TODO: Mixed?
            0xff16 => self.nr21 = value, // TODO: Mixed?
            0xff17 => self.nr22 = value,
            0xff18 => self.nr23 = value,
            0xff19 => self.nr24 = value, // TODO: Mixed?
            0xff1a => self.nr30 = value,
            0xff1b => self.nr31 = value,
            0xff1c => self.nr32 = value,
            0xff1d => self.nr33 = value,
            0xff1e => self.nr34 = value, // TODO: Mixed?
            0xff20 => self.nr41 = value,
            0xff21 => self.nr42 = value,
            0xff22 => self.nr43 = value,
            0xff23 => self.nr44 = value, // TODO: Mixed?
            0xff24 => self.nr50 = value,
            0xff25 => self.nr51 = value,
            0xff26 => self.nr52 = value, // TODO: Mixed?
            0xff30..=0xff3f => self.wave_ram[(addr - 0xff30) as usize] = value,
            0xff40 => self.lcdc = value,
            0xff41 => self.stat = value & 0b1111_1000,
            0xff42 => self.scy = value,
            0xff43 => self.scx = value,
            0xff44 => {} // panic!("cannot write ly register"),
            0xff45 => self.lyc = value,
            0xff46 => self.dma = value,
            0xff47 => self.bgp = value,
            0xff48 => self.obp0 = value,
            0xff49 => self.obp1 = value,
            0xff4a => self.wy = value,
            0xff4b => self.wx = value,
            0xff4d => {}
            0xff4f => {}
            0xff51 => self.hdma1 = value,
            0xff52 => self.hdma2 = value,
            0xff53 => self.hdma3 = value,
            0xff54 => self.hdma4 = value,
            0xff55 => self.hdma5 = value,
            0xff56 => {} // self.rp = value,
            0xff68 => {} // self.bcps = value,
            0xff69 => {} // self.bcpd = value,
            0xff6a => {} // self.ocps = value,
            0xff6b => {} // self.ocpd = value,
            0xff6c => {} // self.opri = value,
            0xff70 => {} // self.svbk = value,
            0xff76 => {} // panic!("cgb only"),
            0xff77 => {} // panic!("cgb only"),
            0xffff => self.interrupt_enable = InterruptFlags::from_bits_truncate(value),
            _ => {} // panic!("invalid IO register address")
        };
    }
}

impl IoRegisters {
    pub fn new() -> Self {
        Self {
            // https://gbdev.io/pandocs/Power_Up_Sequence.html
            joyp: 0xcf,
            sb: 0x00,
            sc: 0x7e,
            div: 0xab,
            cpu_clock: 0,
            tima: 0x00,
            clock_accumulator: 0,
            tma: 0x00,
            tac: 0xf8,
            interrupt_flag: InterruptFlags::from_bits_retain(0xe1),
            nr10: 0x80,
            nr11: 0xbf,
            nr12: 0xf3,
            nr13: 0xff,
            nr14: 0xbf,
            nr21: 0xbf,
            nr22: 0x00,
            nr23: 0xff,
            nr24: 0xbf,
            nr30: 0x7f,
            nr31: 0xff,
            nr32: 0x9f,
            nr33: 0xff,
            nr34: 0xbf,
            nr41: 0xff,
            nr42: 0x00,
            nr43: 0x00,
            nr44: 0xbf,
            nr50: 0x77,
            nr51: 0xf3,
            nr52: 0xf1,
            wave_ram: [0; 0x10],
            lcdc: 0x91,
            stat: 0x85,
            scy: 0x00,
            scx: 0x00,
            ly: 0x00,
            lyc: 0x00,
            dma: 0xff,
            bgp: 0xfc,
            obp0: 0x00,
            obp1: 0x00,
            wy: 0x00,
            wx: 0x00,
            key1: 0xff,
            vbk: 0xff,
            hdma1: 0xff,
            hdma2: 0xff,
            hdma3: 0xff,
            hdma4: 0xff,
            hdma5: 0xff,
            rp: 0xff,
            bcps: 0xff,
            bcpd: 0xff,
            ocps: 0xff,
            ocpd: 0xff,
            opri: 0xff, // Unknown value on power-up. Extrapolating.
            svbk: 0xff,
            interrupt_enable: InterruptFlags::from_bits_retain(0x00),
        }
    }
}
