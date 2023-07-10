use bitflags::Flags;
use super::{
    io_registers::IoRegisters,
    Mem,
};

const APU_FREQUENCY: usize = 1024 * 1024; // Hz

pub(crate) const AUDIO_SAMPLE_RATE: usize = 48_000;

// NOTE: This value is actually more-or-less arbitrary. It just worked. Using half of it caused audio popping, using double caused frames to take too long.
//  Using a value calculated based on expected frame rate resulted in roughly the same results.
pub(crate) const AUDIO_BUFFER_SIZE: usize = 1024 * 2;

bitflags! {
    /// Sound panning
    /// Bit 7 - Mix channel 4 into left output
    /// Bit 6 - Mix channel 3 into left output
    /// Bit 5 - Mix channel 2 into left output
    /// Bit 4 - Mix channel 1 into left output
    /// Bit 3 - Mix channel 4 into right output
    /// Bit 2 - Mix channel 3 into right output
    /// Bit 1 - Mix channel 2 into right output
    /// Bit 0 - Mix channel 1 into right output
    pub struct SoundPanning : u8 {
        const CH1_RIGHT = (1 << 0);
        const CH2_RIGHT = (1 << 1);
        const CH3_RIGHT = (1 << 2);
        const CH4_RIGHT = (1 << 3);
        const CH1_LEFT = (1 << 4);
        const CH2_LEFT = (1 << 5);
        const CH3_LEFT = (1 << 6);
        const CH4_LEFT = (1 << 7);
    }
}

bitflags! {
    /// Sound on/off
    /// Bit 7 - All sound on/off  (0: turn the APU off) (Read/Write)
    /// Bit 3 - Channel 4 ON flag (Read Only)
    /// Bit 2 - Channel 3 ON flag (Read Only)
    /// Bit 1 - Channel 2 ON flag (Read Only)
    /// Bit 0 - Channel 1 ON flag (Read Only)
    pub struct SoundEnable : u8 {
        const CH1_ENABLE = (1 << 0);
        const CH2_ENABLE = (1 << 1);
        const CH3_ENABLE = (1 << 2);
        const CH4_ENABLE = (1 << 3);
        const SOUND_ENABLE = (1 << 7);
    }
}

pub struct Apu {
    accumulator: f32,
    pub buffer: Vec<f32>,
    pub master_volume: f32,
    pub sample_rate: usize,
    div_prev: u8,
    pub div_apu: u8,
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
    ch1_freq_sweep_addition: bool,
    ch1_freq_sweep_slope: u8,
    ch1_freq_sweep_pace: u8,
    ch1_freq_sweep_counter: u8,
    ch1_length_timer: u8,
    ch1_envelope_sweep_pace: u8,
    ch1_envelope_sweep_counter: u8,
    ch1_envelope_sweep_direction_increase: i8,
    ch1_period_counter: u16,
    ch1_duty_counter: u8,
    ch1_volume: u8,
    /// Channel 2 sweep
    pub nr21: u8,
    /// Channel 2 length timer & duty cycle
    pub nr22: u8,
    /// Channel 2 volume & envelope
    pub nr23: u8,
    /// Channel 2 wavelength high & control
    pub nr24: u8,
    ch2_length_timer: u8,
    ch2_envelope_sweep_pace: u8,
    ch2_envelope_sweep_counter: u8,
    ch2_envelope_sweep_direction_increase: i8,
    ch2_period_counter: u16,
    ch2_duty_counter: u8,
    ch2_volume: u8,
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
    ch3_length_timer: u8,
    ch3_period_counter: u16,
    ch3_sample_counter: u8,
    /// Channel 4 sweep
    pub nr41: u8,
    /// Channel 4 length timer & duty cycle
    pub nr42: u8,
    /// Channel 4 volume & envelope
    pub nr43: u8,
    /// Channel 4 wavelength high & control
    pub nr44: u8,
    ch4_length_timer: u8,
    ch4_envelope_sweep_pace: u8,
    ch4_envelope_sweep_counter: u8,
    ch4_envelope_sweep_direction_increase: i8,
    ch4_tick_counter: usize,
    ch4_lsfr: u16,
    ch4_volume: u8,
    /// Master volume & VIN panning
    /// Bit 7   - Mix VIN into left output  (1=Enable)
    /// Bit 6-4 - Left output volume        (0-7)
    /// Bit 3   - Mix VIN into right output (1=Enable)
    /// Bit 2-0 - Right output volume       (0-7)
    pub nr50: u8,
    pub nr51: SoundPanning,
    pub nr52: SoundEnable,
    /// Wave pattern RAM
    pub wave_ram: [u8; 0x10],
}

impl Apu {
    pub fn new() -> Self {
        Self {
            accumulator: 0.0,
            buffer: Vec::<f32>::with_capacity(AUDIO_BUFFER_SIZE),
            master_volume: 0.25,
            sample_rate: AUDIO_SAMPLE_RATE,
            div_prev: 0,
            div_apu: 0,
            nr10: 0x80,
            nr11: 0xbf,
            nr12: 0xf3,
            nr13: 0xff,
            nr14: 0xbf,
            ch1_freq_sweep_addition: false, // bit 3 of nr10
            ch1_freq_sweep_slope: 0, // bit 0-2 of nr10
            ch1_freq_sweep_pace: 0, // bits 4-6 of nr10
            ch1_freq_sweep_counter: 0,
            ch1_length_timer: 0x1f, // bits 0-5 of nr11
            ch1_envelope_sweep_pace: 3, // bit 0-2 of nr12
            ch1_envelope_sweep_counter: 0,
            ch1_envelope_sweep_direction_increase: -1, // 1 if bit 3 of nr12 is 1, otherwise -1
            ch1_period_counter: 0x7ff, // (nr14 & 3) << 8 | nr13
            ch1_duty_counter: 0, // When first starting up a pulse channel, it will always output a (digital) zero.
            ch1_volume: 0xf, // bit 4-7 of nr12
            nr21: 0xbf,
            nr22: 0x00,
            nr23: 0xff,
            nr24: 0xbf,
            ch2_length_timer: 0x1f,
            ch2_envelope_sweep_pace: 3,
            ch2_envelope_sweep_counter: 0,
            ch2_envelope_sweep_direction_increase: -1,
            ch2_period_counter: 0x7ff,
            ch2_duty_counter: 0, // When first starting up a pulse channel, it will always output a (digital) zero.
            ch2_volume: 0xf,
            nr30: 0x7f,
            nr31: 0xff,
            nr32: 0x9f,
            nr33: 0xff,
            nr34: 0xbf,
            ch3_length_timer: 0xff,
            ch3_period_counter: 0x7ff,
            ch3_sample_counter: 0,
            nr41: 0xff,
            nr42: 0x00,
            nr43: 0x00,
            nr44: 0xbf,
            ch4_length_timer: 0x1f,
            ch4_envelope_sweep_pace: 0,
            ch4_envelope_sweep_counter: 0,
            ch4_envelope_sweep_direction_increase: -1,
            ch4_tick_counter: 0,
            ch4_lsfr: 0,
            ch4_volume: 0,
            nr50: 0x77,
            nr51: SoundPanning::from_bits_retain(0xf3),
            nr52: SoundEnable::from_bits_retain(0xf1),
            wave_ram: [0; 0x10],
        }
    }

    pub fn tick(&mut self, registers: &IoRegisters) {
        // TODO: if NR52.7 is off, all registers except NR52 and NRx1 are read-only. There is a different case for GBC.

        if self.div_prev & (1 << 4) != 0 && registers.div & (1 << 4) == 0 {
            self.div_apu = self.div_apu.wrapping_add(1);

            self.process();
        }

        // Pulse modulation
        {
            self.ch1_period_counter = (self.ch1_period_counter + 1) & 0x7ff;
            if self.ch1_period_counter == 0 {
                let period: u16 = (self.nr14 as u16 & 0b0000_0111) << 8 | self.nr13 as u16;

                self.ch1_period_counter = period;

                self.ch1_duty_counter = (self.ch1_duty_counter + 1) % 8;
            }

            self.ch2_period_counter = (self.ch2_period_counter + 1) & 0x7ff;
            if self.ch2_period_counter == 0 {
                let period: u16 = (self.nr24 as u16 & 0b0000_0111) << 8 | self.nr23 as u16;

                self.ch2_period_counter = period;

                self.ch2_duty_counter = (self.ch2_duty_counter + 1) % 8;
            }
        }

        // Wave output
        {
            // Clocked at 2x APU_FREQUENCY
            for _ in 0..2 {
                self.ch3_period_counter = (self.ch3_period_counter + 1) & 0x7ff;
                if self.ch3_period_counter == 0 {
                    let period: u16 = (self.nr34 as u16 & 0b0000_0111) << 8 | self.nr33 as u16;

                    self.ch3_period_counter = period;

                    self.ch3_sample_counter = (self.ch3_sample_counter + 1) % 32;
                }
            }
        }

        // Noise
        {
            let clock_shift = self.nr43 >> 4;
            let lsfr_short_mode = self.nr43 & (1 << 3) == 1;
            let clock_divider = self.nr43 & 0b0000_0111;

            let tick_frequency_denominator = 1 << clock_shift;
            let tick_frequency_denominator = if clock_divider == 0 {
                // 0 is treated as 0.5, so divide by 2.
                tick_frequency_denominator as f32 * 0.5
            } else {
                (clock_divider * tick_frequency_denominator) as f32
            };

            let tick_frequency = (262_144f32 / tick_frequency_denominator) as usize;
            let tick_max_count = APU_FREQUENCY / tick_frequency;

            self.ch4_tick_counter = (self.ch4_tick_counter + 1) % tick_max_count;
            if self.ch4_tick_counter == 0 {
                let next_bit = !((self.ch4_lsfr & 1) ^ ((self.ch4_lsfr >> 1) & 1)) & 1;

                self.ch4_lsfr = self.ch4_lsfr & 0x7fff; // Turn off bit 15
                self.ch4_lsfr |= next_bit << 15; // Write bit 15

                // Also write bit 7
                if lsfr_short_mode {
                    self.ch4_lsfr = self.ch4_lsfr & 0xff7f; // Turn off bit 7
                    self.ch4_lsfr |= next_bit << 7; // Write bit 7
                }

                self.ch4_lsfr >>= 1;
            }
        }

        fn sample_to_volume(sample: u8) -> f32 {
            ((0xf - sample) as f32 / 0xf as f32) * 2.0 - 1.0
        }

        // Mixing
        let step = APU_FREQUENCY as f32 / self.sample_rate as f32;
        while self.accumulator > step {
            // Channel 1
            let ch1_dac_enabled = self.nr12 & 0xf8 != 0;
            let ch1_sample = if ch1_dac_enabled && self.nr52.contains(SoundEnable::CH1_ENABLE) {
                let wave_duty = match self.nr11 >> 6 {
                    0 => 1, // 12.5% of 8 samples
                    1 => 2, // 25% of 8 samples
                    2 => 4, // 50% of 8 samples
                    3 => 6, // 75% of 8 samples
                    _ => unreachable!()
                };

                let sample = if self.ch1_duty_counter < wave_duty {
                    self.ch1_volume
                } else {
                    0
                };

                sample
            } else {
                0
            };

            // Channel 2
            let ch2_dac_enabled = self.nr22 & 0xf8 != 0;
            let ch2_sample = if ch2_dac_enabled && self.nr52.contains(SoundEnable::CH2_ENABLE) {
                // Push one sample
                let wave_duty = match self.nr21 >> 6 {
                    0 => 1, // 12.5% of 8 samples
                    1 => 2, // 25% of 8 samples
                    2 => 4, // 50% of 8 samples
                    3 => 6, // 75% of 8 samples
                    _ => unreachable!()
                };

                let sample = if self.ch2_duty_counter < wave_duty {
                    self.ch2_volume
                } else {
                    0
                };

                sample
            } else {
                0
            };

            // Channel 3
            let ch3_dac_enabled = self.nr30 & (1 << 7) != 0;
            let ch3_sample = if ch3_dac_enabled && self.nr52.contains(SoundEnable::CH3_ENABLE) {
                let wave_sample_pair = self.mem_read(0xff30 + (self.ch3_sample_counter >> 1) as u16);
                let wave_sample = if self.ch3_sample_counter % 2 == 0 {
                    wave_sample_pair >> 4
                } else {
                    wave_sample_pair & 0xf
                };

                let output_level = match (self.nr32 >> 5) & 0x3 {
                    0 => 0,
                    1 => wave_sample,
                    2 => wave_sample >> 1,
                    3 => wave_sample >> 2,
                    _ => unreachable!()
                };

                output_level
            } else {
                0
            };

            // Channel 4
            let ch4_dac_enabled = self.nr42 & 0xf8 != 0;
            let ch4_sample = if ch4_dac_enabled && self.nr52.contains(SoundEnable::CH4_ENABLE) && (self.ch4_lsfr & 1) != 0 {
                self.ch4_volume
            } else {
                0
            };

            let sample_left =
                sample_to_volume(ch1_sample) * self.nr51.contains(SoundPanning::CH1_LEFT) as u8 as f32 +
                    sample_to_volume(ch2_sample) * self.nr51.contains(SoundPanning::CH2_LEFT) as u8 as f32 +
                    sample_to_volume(ch3_sample) * self.nr51.contains(SoundPanning::CH3_LEFT) as u8 as f32 +
                    sample_to_volume(ch4_sample) * self.nr51.contains(SoundPanning::CH4_LEFT) as u8 as f32;
            let sample_right =
                sample_to_volume(ch1_sample) * self.nr51.contains(SoundPanning::CH1_RIGHT) as u8 as f32 +
                    sample_to_volume(ch2_sample) * self.nr51.contains(SoundPanning::CH2_RIGHT) as u8 as f32 +
                    sample_to_volume(ch3_sample) * self.nr51.contains(SoundPanning::CH3_RIGHT) as u8 as f32 +
                    sample_to_volume(ch4_sample) * self.nr51.contains(SoundPanning::CH4_RIGHT) as u8 as f32;

            let volume_left = (1 + ((self.nr50 >> 4) & 7)) as f32 * 0.125;
            let volume_right = (1 + ((self.nr50 >> 0) & 7)) as f32 * 0.125;

            self.buffer.push(sample_left * volume_left * 0.25 * self.master_volume);
            self.buffer.push(sample_right * volume_right * 0.25 * self.master_volume);

            self.accumulator -= step;
        }

        self.accumulator += 1.0;

        self.div_prev = registers.div;
    }

    fn process(&mut self) {
        // Envelope sweep
        // 64Hz
        if self.div_apu % 8 == 0 {
            // Channel 1
            {
                if self.nr52.contains(SoundEnable::CH1_ENABLE) && self.ch1_envelope_sweep_pace > 0 {
                    self.ch1_envelope_sweep_counter = (self.ch1_envelope_sweep_counter + 1) % self.ch1_envelope_sweep_pace;
                    if self.ch1_envelope_sweep_pace > 0 && self.ch1_envelope_sweep_counter == 0 {
                        self.ch1_volume = self.ch1_volume.saturating_add_signed(self.ch1_envelope_sweep_direction_increase).min(0xf);
                    }
                }
            }

            // Channel 2
            {
                if self.nr52.contains(SoundEnable::CH2_ENABLE) && self.ch2_envelope_sweep_pace > 0 {
                    self.ch2_envelope_sweep_counter = (self.ch2_envelope_sweep_counter + 1) % self.ch2_envelope_sweep_pace;
                    if self.ch2_envelope_sweep_counter == 0 {
                        self.ch2_volume = self.ch2_volume.saturating_add_signed(self.ch2_envelope_sweep_direction_increase).min(0xf);
                    }

                    let ch2_sweep_pace = self.nr22 & 0b0000_0111;
                    self.ch2_envelope_sweep_pace = ch2_sweep_pace;
                }
            }

            // Channel 4
            {
                if self.nr52.contains(SoundEnable::CH4_ENABLE) && self.ch4_envelope_sweep_pace > 0 {
                    self.ch4_envelope_sweep_counter = (self.ch4_envelope_sweep_counter + 1) % self.ch4_envelope_sweep_pace;
                    if self.ch4_envelope_sweep_counter == 0 {
                        self.ch4_volume = self.ch4_volume.saturating_add_signed(self.ch4_envelope_sweep_direction_increase).min(0xf);
                    }

                    let ch4_sweep_pace = self.nr42 & 0b0000_0111;
                    self.ch4_envelope_sweep_pace = ch4_sweep_pace;
                }
            }
        }

        // Sound length
        // 256Hz
        if self.div_apu % 2 == 0 {
            let ch1_length_timer_enable = self.nr14 & (1 << 6) != 0;
            if ch1_length_timer_enable {
                self.ch1_length_timer = self.ch1_length_timer.wrapping_sub(1);

                if self.ch1_length_timer == 0 {
                    // Turn off channel 1
                    self.nr52.remove(SoundEnable::CH1_ENABLE);
                }
            }

            let ch2_length_timer_enable = self.nr24 & (1 << 6) != 0;
            if ch2_length_timer_enable {
                self.ch2_length_timer = self.ch2_length_timer.wrapping_sub(1);

                if self.ch2_length_timer == 0 {
                    // Turn off channel 2
                    self.nr52.remove(SoundEnable::CH2_ENABLE);
                }
            }

            let ch3_length_timer_enable = self.nr34 & (1 << 6) != 0;
            if ch3_length_timer_enable {
                self.ch3_length_timer = self.ch3_length_timer.wrapping_sub(1);

                if self.ch3_length_timer == 0 {
                    // Turn off channel 3
                    self.nr52.remove(SoundEnable::CH3_ENABLE);
                }
            }

            let ch4_length_timer_enable = self.nr44 & (1 << 6) != 0;
            if ch4_length_timer_enable {
                // Turn off channel 4
                self.ch4_length_timer = self.ch4_length_timer.wrapping_sub(1);

                if self.ch4_length_timer == 0 {
                    self.nr52.remove(SoundEnable::CH4_ENABLE);
                }
            }
        }

        // Channel 1 frequency sweep
        // 128Hz
        if self.div_apu % 4 == 0 && self.ch1_freq_sweep_pace != 0 {
            self.ch1_freq_sweep_counter = (self.ch1_freq_sweep_counter + 1) % self.ch1_freq_sweep_pace;

            if self.ch1_freq_sweep_counter == 0 {
                let period: u16 = (self.nr14 as u16 & 0b0000_0111) << 8 | self.nr13 as u16;

                let next_period = if self.ch1_freq_sweep_addition {
                    period + (period >> self.ch1_freq_sweep_slope)
                } else {
                    period - (period >> self.ch1_freq_sweep_slope)
                };

                self.nr13 = next_period as u8;
                self.nr14 = self.nr14 & 0b0000_0111 | (next_period >> 8) as u8;

                if next_period > 0x7ff {
                    self.nr52.remove(SoundEnable::CH1_ENABLE);
                }

                self.ch1_freq_sweep_pace = (self.nr10 & 0b0111_0000) >> 4;
                self.ch1_freq_sweep_addition = self.nr10 & 0b0000_1000 == 0;
                self.ch1_freq_sweep_slope = self.nr10 & 0b0000_0111;
            }
        }
    }

    pub fn extract_audio_buffer(&mut self) -> Vec<f32> {
        return std::mem::replace(&mut self.buffer, Vec::with_capacity(AUDIO_BUFFER_SIZE));
    }
}

impl Mem for Apu {
    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0xff10 => self.nr10,
            0xff11 => self.nr11 & 0b1100_0000,
            0xff12 => self.nr12,
            0xff13 => panic!("cannot read nr13 register"),
            0xff14 => self.nr14 & (1 << 6),
            0xff16 => self.nr21 & 0b1100_0000,
            0xff17 => self.nr22,
            0xff18 => panic!("cannot read nr23 register"),
            0xff19 => self.nr24 & (1 << 6),
            0xff1a => self.nr30,
            0xff1b => panic!("cannot read nr31 register"),
            0xff1c => self.nr32,
            0xff1d => panic!("cannot read nr33 register"),
            0xff1e => self.nr34 & (1 << 6),
            0xff20 => panic!("cannot read nr41 register"),
            0xff21 => self.nr42,
            0xff22 => self.nr43,
            0xff23 => self.nr44 & (1 << 6),
            0xff24 => self.nr50,
            0xff25 => self.nr51.bits(),
            0xff26 => self.nr52.bits(),
            0xff30..=0xff3f => self.wave_ram[(addr - 0xff30) as usize],
            _ => unreachable!()
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0xff10 => self.nr10 = value,
            0xff11 => {
                self.nr11 = (value & 0b1100_0000) & (63 - (value & 0b0011_1111)); // Length timer is inverted when written and counts down.
                self.ch1_length_timer = self.nr11 & 0b0011_1111;
            }
            0xff12 => self.nr12 = value,
            0xff13 => self.nr13 = value,
            0xff14 => {
                self.nr14 = value;

                let ch1_dac_enable = self.nr12 & 0xf8 != 0;
                if value & (1 << 7) != 0 && ch1_dac_enable {
                    self.ch1_length_timer = self.nr11 & 0b0011_1111;
                    self.ch1_freq_sweep_pace = (self.nr10 & 0b0111_0000) >> 4;
                    self.ch1_freq_sweep_addition = self.nr10 & 0b0000_1000 == 0;
                    self.ch1_freq_sweep_slope = self.nr10 & 0b0000_0111;
                    self.ch1_freq_sweep_counter = 0;
                    self.ch1_envelope_sweep_direction_increase = if self.nr12 & 0b0000_1000 == 0 { -1 } else { 1 };
                    self.ch1_envelope_sweep_pace = self.nr12 & 0b0000_0011;
                    self.ch1_envelope_sweep_counter = 0;
                    self.ch1_duty_counter = 0;
                    self.ch1_volume = self.nr12 >> 4;

                    self.nr52.insert(SoundEnable::CH1_ENABLE);
                }
            }
            0xff16 => {
                self.nr21 = (value & 0b1100_0000) & (63 - (value & 0b0011_1111)); // Length timer is inverted when written and counts down.
                self.ch2_length_timer = self.nr21 & 0b0011_1111;
            }
            0xff17 => self.nr22 = value,
            0xff18 => self.nr23 = value,
            0xff19 => {
                self.nr24 = value;

                let ch2_dac_enable = self.nr22 & 0xf8 != 0;
                if value & (1 << 7) != 0 && ch2_dac_enable {
                    self.ch2_length_timer = self.nr21 & 0b0011_1111;
                    self.ch2_envelope_sweep_direction_increase = if self.nr22 & 0b0000_1000 == 0 { -1 } else { 1 };
                    self.ch2_envelope_sweep_pace = self.nr22 & 0b0000_0011;
                    self.ch2_envelope_sweep_counter = 0;
                    self.ch2_duty_counter = 0;
                    self.ch2_volume = self.nr22 >> 4;

                    self.nr52.insert(SoundEnable::CH2_ENABLE);
                }
            }
            0xff1a => self.nr30 = value & (1 << 7),
            0xff1b => self.nr31 = 255 - value,
            0xff1c => self.nr32 = value,
            0xff1d => self.nr33 = value,
            0xff1e => {
                self.nr34 = value;

                let ch3_dac_enable = self.nr30 & (1 << 7) != 0;
                if value & (1 << 7) != 0 && ch3_dac_enable {
                    self.ch3_length_timer = self.nr31;
                    self.ch3_period_counter = 0;
                    self.ch3_sample_counter = 0;

                    self.nr52.insert(SoundEnable::CH3_ENABLE);
                }
            }
            0xff20 => {
                self.nr41 = 63 - (value & 0b0011_1111); // Length timer is inverted when written and counts down.
                self.ch4_length_timer = self.nr41 & 0b0011_1111;
            }
            0xff21 => self.nr42 = value,
            0xff22 => self.nr43 = value,
            0xff23 => {
                self.nr44 = value;

                let ch4_dac_enable = self.nr42 & 0xf8 != 0;
                if value & (1 << 7) != 0 && ch4_dac_enable {
                    self.ch4_length_timer = self.nr41 & 0b0011_1111;
                    self.ch4_envelope_sweep_direction_increase = if self.nr42 & 0b0000_1000 == 0 { -1 } else { 1 };
                    self.ch4_envelope_sweep_pace = self.nr42 & 0b0000_0011;
                    self.ch4_envelope_sweep_counter = 0;
                    self.ch4_tick_counter = 0;
                    self.ch4_lsfr = 0;
                    self.ch4_volume = self.nr42 >> 4;

                    self.nr52.insert(SoundEnable::CH4_ENABLE);
                }
            }
            0xff24 => self.nr50 = value,
            0xff25 => self.nr51 = SoundPanning::from_bits_retain(value),
            0xff26 => self.nr52 = SoundEnable::from_bits_retain(value & (1 << 7)),
            0xff30..=0xff3f => self.wave_ram[(addr - 0xff30) as usize] = value,
            _ => unreachable!()
        }
    }
}