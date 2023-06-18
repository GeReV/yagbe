use crate::io_registers::IoRegisters;
use crate::Mem;

pub struct Apu {
    div_prev: u8,
    pub div_apu: u8,
    freq_sweep_pace: u8,
    freq_sweep_counter: u8,
    /// Channel 1 sweep
    /// Bit 6-4 - Sweep pace
    /// Bit 3   - Sweep increase/decrease
    ///            0: Addition    (wavelength increases)
    ///            1: Subtraction (wavelength decreases)
    /// Bit 2-0 - Sweep slope control (n: 0-7)
    pub nr10: u8,
    /// Channel 1 length timer & duty cycle
    /// Bit 7-6 - Wave duty            (Read/Write)
    /// Bit 5-0 - Initial length timer (Write Only)
    pub nr11: u8,
    ch1_envelope_sweep_pace: u8,
    ch1_envelope_sweep_counter: u8,
    /// Channel 1 volume & envelope
    /// Bit 7-4 - Initial volume of envelope (0-F) (0=No Sound)
    /// Bit 3   - Envelope direction (0=Decrease, 1=Increase)
    /// Bit 2-0 - Sweep pace (0=No Sweep)
    pub nr12: u8,
    /// Channel 1 wavelength low [write-only]
    pub nr13: u8,
    /// Channel 1 wavelength high & control
    /// Bit 7   - Trigger (1=Restart channel)  (Write Only)
    /// Bit 6   - Sound Length enable          (Read/Write)
    ///           (1=Stop output when length in NR11 expires)
    /// Bit 2-0 - "Wavelength"'s higher 3 bits (Write Only)
    pub nr14: u8,
    /// Channel 2 sweep
    pub nr21: u8,
    ch2_envelope_sweep_pace: u8,
    ch2_envelope_sweep_counter: u8,
    /// Channel 2 length timer & duty cycle
    pub nr22: u8,
    /// Channel 2 volume & envelope
    pub nr23: u8,
    /// Channel 2 wavelength high & control
    pub nr24: u8,
    /// Channel 3 DAC enable
    /// Bit 7 - Sound Channel 3 DAC  (0=Off, 1=On)
    pub nr30: u8,
    /// Channel 3 length timer [write-only]
    /// Bit 7-0 - length timer
    pub nr31: u8,
    /// Channel 3 output level
    /// Bits 6-5 - Output level selection
    pub nr32: u8,
    /// Channel 3 wavelength low [write-only]
    pub nr33: u8,
    /// Channel 3 wavelength high & control
    pub nr34: u8,
    /// Channel 4 sweep
    pub nr41: u8,
    ch4_envelope_sweep_pace: u8,
    ch4_envelope_sweep_counter: u8,
    /// Channel 4 length timer & duty cycle
    pub nr42: u8,
    /// Channel 4 volume & envelope
    pub nr43: u8,
    /// Channel 4 wavelength high & control
    pub nr44: u8,
    /// Master volume & VIN panning
    /// Bit 7   - Mix VIN into left output  (1=Enable)
    /// Bit 6-4 - Left output volume        (0-7)
    /// Bit 3   - Mix VIN into right output (1=Enable)
    /// Bit 2-0 - Right output volume       (0-7)
    pub nr50: u8,
    /// Sound panning
    /// Bit 7 - Mix channel 4 into left output
    /// Bit 6 - Mix channel 3 into left output
    /// Bit 5 - Mix channel 2 into left output
    /// Bit 4 - Mix channel 1 into left output
    /// Bit 3 - Mix channel 4 into right output
    /// Bit 2 - Mix channel 3 into right output
    /// Bit 1 - Mix channel 2 into right output
    /// Bit 0 - Mix channel 1 into right output
    pub nr51: u8,
    /// Sound on/off
    /// Bit 7 - All sound on/off  (0: turn the APU off) (Read/Write)
    /// Bit 3 - Channel 4 ON flag (Read Only)
    /// Bit 2 - Channel 3 ON flag (Read Only)
    /// Bit 1 - Channel 2 ON flag (Read Only)
    /// Bit 0 - Channel 1 ON flag (Read Only)
    pub nr52: u8,
    /// Wave pattern RAM
    pub wave_ram: [u8; 0x10],
}

impl Apu {
    pub  fn new() -> Self {
        Self {
            div_prev: 0,
            div_apu: 0,
            freq_sweep_pace: 0,
            freq_sweep_counter: 0,
            nr10: 0x80,
            nr11: 0xbf,
            ch1_envelope_sweep_pace: 3, // bit 0-2 of nr12
            ch1_envelope_sweep_counter: 0,
            nr12: 0xf3,
            nr13: 0xff,
            nr14: 0xbf,
            nr21: 0xbf,
            ch2_envelope_sweep_pace: 0,
            ch2_envelope_sweep_counter: 0,
            nr22: 0x00,
            nr23: 0xff,
            nr24: 0xbf,
            nr30: 0x7f,
            nr31: 0xff,
            nr32: 0x9f,
            nr33: 0xff,
            nr34: 0xbf,
            nr41: 0xff,
            ch4_envelope_sweep_pace: 0,
            ch4_envelope_sweep_counter: 0,
            nr42: 0x00,
            nr43: 0x00,
            nr44: 0xbf,
            nr50: 0x77,
            nr51: 0xf3,
            nr52: 0xf1,
            wave_ram: [0; 0x10],
        }
    }
    
    pub fn tick(&mut self, registers: &IoRegisters) {
        // TODO: if NR52.7 is off, all registers except NR52 and NRx1 are read-only. There is a different case for GBC.
        
        if self.div_prev & (1<<4) != 0 && registers.div & (1<<4) == 0 {
            self.div_apu = self.div_prev.wrapping_add(1);
        }

        // Envelope sweep
        // 64Hz
        if self.div_apu % 8 == 0 {
            // Channel 1
            {
                if self.nr12 & 0b1111_1000 == 0 {
                    self.nr52 &= !(1 << 0);
                } else {
                    let ch1_initial_volume = self.nr12 >> 4;
                    let ch1_envelope_direction_increase = self.nr12 & 0b0000_1000 == 0;
                    let ch1_sweep_pace = self.nr12 & 0b0000_0111;

                    self.ch1_envelope_sweep_counter = (self.ch1_envelope_sweep_counter + 1) % self.ch1_envelope_sweep_pace;
                    if self.ch1_envelope_sweep_counter == 0 {
                        // TODO: Apply envelope.
                    }

                    self.ch1_envelope_sweep_pace = ch1_sweep_pace;
                }
            }

            // Channel 2
            {
                if self.nr22 & 0b1111_1000 == 0 {
                    self.nr52 &= !(1<<1);
                } else {
                    let ch2_initial_volume = self.nr22 >> 4;
                    let ch2_envelope_direction_increase = self.nr22 & 0b0000_1000 == 0;
                    let ch2_sweep_pace = self.nr22 & 0b0000_0111;

                    self.ch2_envelope_sweep_counter = (self.ch2_envelope_sweep_counter + 1) % self.ch2_envelope_sweep_pace;
                    if self.ch2_envelope_sweep_counter == 0 {
                        // TODO: Apply envelope.
                    }

                    self.ch2_envelope_sweep_pace = ch2_sweep_pace;
                }
            }

            // Channel 4
            {
                if self.nr42 & 0b1111_1000 == 0 {
                    self.nr52 &= !(1<<3);
                } else {
                    let ch4_initial_volume = self.nr42 >> 4;
                    let ch4_envelope_direction_increase = self.nr42 & 0b0000_1000 == 0;
                    let ch4_sweep_pace = self.nr42 & 0b0000_0111;

                    self.ch4_envelope_sweep_counter = (self.ch4_envelope_sweep_counter + 1) % self.ch4_envelope_sweep_pace;
                    if self.ch4_envelope_sweep_counter == 0 {
                        // TODO: Apply envelope.
                    }

                    self.ch4_envelope_sweep_pace = ch4_sweep_pace;
                }
            }
        }

        // Sound length
        // 256Hz
        if self.div_apu % 2 == 0 {
            let ch1_length_timer_enable = self.nr14 & (1<<6) != 0;
            if ch1_length_timer_enable {
                self.nr11 = (self.nr11 & 0b0011_1111).wrapping_sub(1) % 64;
                
                if self.nr11 == 0 {
                    // Turn off channel 1
                    self.nr52 &= !(1 << 0);
                }
            }

            let ch2_length_timer_enable = self.nr24 & (1<<6) != 0;
            if ch2_length_timer_enable {
                self.nr21 = (self.nr21 & 0b0011_1111).wrapping_sub(1) % 64;

                if self.nr21 == 0 {
                    self.nr52 &= !(1 << 1); 
                }
            }

            let ch3_length_timer_enable = self.nr34 & (1<<6) != 0;
            if ch3_length_timer_enable {
                self.nr31 = self.nr31.wrapping_sub(1);

                if self.nr31 == 0 {
                    self.nr52 &= !(1 << 2);
                }
            }

            let ch4_length_timer_enable = self.nr41 & (1<<6) != 0;
            if ch4_length_timer_enable {
                self.nr41 = self.nr41.wrapping_sub(1);

                if self.nr41 == 0 {
                    self.nr52 &= !(1 << 4); 
                }
            }
        }
        
        // Channel 1 frequency sweep
        // 128Hz
        let sweep_pace = (self.nr10 & 0b0111_0000) >> 4;
        if self.div_apu % 4 == 0 && sweep_pace != 0 {
            let sweep_addition = self.nr10 & 0b0000_1000 == 0;
            let sweep_slope = self.nr10 & 0b0000_0111;
            
            self.freq_sweep_counter = (self.freq_sweep_counter + 1) % self.freq_sweep_pace;
            
            if self.freq_sweep_counter == 0 {
                let wavelength: u16 = (self.nr14 as u16 & 0b0000_0111) << 8 | self.nr13 as u16;

                let mut next_wavelength = if sweep_addition {
                    wavelength + (wavelength >> sweep_slope)
                } else {
                    wavelength - (wavelength >> sweep_slope)
                };

                if next_wavelength > 0x7ff {
                    self.nr52 &= !(1 << 0);
                }

                next_wavelength &= 0x7ff;

                self.nr13 = next_wavelength as u8;
                self.nr14 = self.nr14 & 0b0000_0111 | (next_wavelength >> 8) as u8;
            }

            self.freq_sweep_pace = sweep_pace;
        }
        
        self.div_prev = registers.div;
    }
}

impl Mem for Apu {
    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0xff10 => self.nr10,
            0xff11 => self.nr11 & 0b1100_0000,
            0xff12 => self.nr12,
            0xff13 => panic!("cannot read nr13 register"),
            0xff14 => self.nr14 & (1<<6),
            0xff16 => self.nr21 & 0b1100_0000,
            0xff17 => self.nr22,
            0xff18 => panic!("cannot read nr23 register"),
            0xff19 => self.nr24 & (1<<6),
            0xff1a => self.nr30,
            0xff1b => panic!("cannot read nr31 register"),
            0xff1c => self.nr32,
            0xff1d => panic!("cannot read nr33 register"),
            0xff1e => self.nr34 & (1<<6),
            0xff20 => panic!("cannot read nr41 register"),
            0xff21 => self.nr42,
            0xff22 => self.nr43,
            0xff23 => self.nr44 & (1<<6),
            0xff24 => self.nr50,
            0xff25 => self.nr51,
            0xff26 => self.nr52,
            0xff30..=0xff3f => self.wave_ram[(addr - 0xff30) as usize],
            _ => unreachable!()
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0xff10 => self.nr10 = value,
            0xff11 => self.nr11 = (value & 0b1100_0000) & (63 - value & 0b0011_1111), // Length timer is inverted when written and counts down.
            0xff12 => self.nr12 = value,
            0xff13 => self.nr13 = value,
            0xff14 => self.nr14 = value,
            0xff16 => self.nr21 =  (value & 0b1100_0000) & (63 - value & 0b0011_1111), // Length timer is inverted when written and counts down.
            0xff17 => self.nr22 = value,
            0xff18 => self.nr23 = value,
            0xff19 => self.nr24 = value,
            0xff1a => self.nr30 = value & (1<<7),
            0xff1b => self.nr31 = 255 - value,
            0xff1c => self.nr32 = value,
            0xff1d => self.nr33 = value,
            0xff1e => self.nr34 = value,
            0xff20 => self.nr41 = (value & 0b1100_0000) & (63 - value & 0b0011_1111), // Length timer is inverted when written and counts down.
            0xff21 => self.nr42 = value,
            0xff22 => self.nr43 = value,
            0xff23 => self.nr44 = value,
            0xff24 => self.nr50 = value,
            0xff25 => self.nr51 = value,
            0xff26 => self.nr52 = value & (1<<7),
            0xff30..=0xff3f => self.wave_ram[(addr - 0xff30) as usize] = value,
            _ => unreachable!()
        }
    }
}