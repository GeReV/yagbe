use bitflags::Flags;
use crate::cpu::Mem;

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

bitflags! {
    #[derive(Default, Copy, Clone, Debug)]
    pub struct LCDControl : u8 {
        const BG_WINDOW_ENABLE = 1 << 0;
        const OBJ_ENABLE = 1 << 1;
        const OBJ_SIZE = 1 << 2; // 0=8x8, 1=8x16
        const BG_TILEMAP_AREA = 1 << 3; // 0=9800-9BFF, 1=9C00-9FFF
        const BG_TILEDATA_AREA = 1 << 4; // 0=8800-97FF, 1=8000-8FFF
        const WINDOW_ENABLE = 1 << 5;
        const WINDOW_TILEMAP_AREA = 1 << 6; // 0=9800-9BFF, 1=9C00-9FFF
        const LCD_PPU_ENABLE = 1 << 7;
    }
}

#[derive(Default)]
pub struct IoRegisters {
    pub joyp_directions: u8,
    pub joyp_actions: u8,
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
    pub lcdc: LCDControl,
    pub stat: u8,
    pub scy: u8,
    pub scx: u8,
    pub ly: u8,
    pub lyc: u8,
    pub dma: u8,
    pub dma_counter: u8,
    pub bgp: u8,
    pub obp0: u8,
    pub obp1: u8,
    pub wy: u8,
    pub window_ly: u8,
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
            0xff40 => self.lcdc.bits(),
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
            0xff00 => {
                // NOTE: Values of JOYP are 0 for selected/pressed, so everything is inversed.
                let prev_joy = self.joyp;
                
                self.joyp = 0b1100_0000 | match value {
                    0x20 => 0x20 | self.joyp_directions,
                    0x10 => 0x10 | self.joyp_actions,
                    _ => self.joyp,
                };
                
                // When either action or direction bits are on, but not both.
                if (self.joyp & 0b0001_0000) ^ (self.joyp & 0b0010_0000) != 0 {
                    for bit in 0..=3 {
                        let mask = 1 << bit;
                        
                        // Joypad interrupt is set whenever joypad bits 0-3 go from high to low, when one of the selection bits (4-5) are set.
                        if (prev_joy & mask) != 0 && (self.joyp & mask) == 0 {
                            self.interrupt_flag.insert(InterruptFlags::JOYPAD);
                            
                            break;
                        }
                    }
                }
            },
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

            // 0xff10..=0xff3f are in the APU.

            0xff40 => self.lcdc = LCDControl::from_bits_retain(value),
            0xff41 => self.stat = value & 0b0111_1000 | 0b1000_0000,
            0xff42 => self.scy = value,
            0xff43 => self.scx = value,
            0xff44 => {} // panic!("cannot write ly register"),
            0xff45 => self.lyc = value,
            0xff46 => {
                self.dma = value;
                self.dma_counter = 160;
            }
            0xff47 => self.bgp = value,
            0xff48 => self.obp0 = value,
            0xff49 => self.obp1 = value,
            0xff4a => self.wy = value,
            0xff4b => {
                self.wx = value;
            }
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
            0xffff => self.interrupt_enable = InterruptFlags::from_bits_retain(0b1110_0000 | value),
            _ => {} // panic!("invalid IO register address")
        };
    }
}

impl IoRegisters {
    pub fn new() -> Self {
        Self {
            // https://gbdev.io/pandocs/Power_Up_Sequence.html
            joyp_directions: 0x0f,
            joyp_actions: 0x0f,
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
            lcdc: LCDControl::from_bits_retain(0x91),
            stat: 0x85,
            scy: 0x00,
            scx: 0x00,
            ly: 0x00,
            lyc: 0x00,
            dma: 0xff,
            dma_counter: 0,
            bgp: 0xfc,
            obp0: 0x00,
            obp1: 0x00,
            wy: 0x00,
            window_ly: 0,
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
