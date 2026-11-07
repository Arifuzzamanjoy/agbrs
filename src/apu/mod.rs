//! Game Boy Advance sound: four PSG channels (GB-compatible) plus the two
//! DirectSound FIFO channels, mixed per GBATEK "Sound Controller".
//!
//! Clocking (CPU = 16.78 MHz):
//! - square duty step  : every (2048 - n) * 16 cycles
//! - wave sample step  : every (2048 - n) * 8 cycles
//! - noise LFSR step   : every (r == 0 ? 8 : 16 * r) << (s + 1) cycles
//! - frame sequencer   : 512 Hz (every 32768 cycles); length on even
//!   steps, sweep on steps 2/6, envelope on step 7.
//! Output rate follows SOUNDBIAS bits 14-15 (GBATEK "amplitude
//! resolution / sampling cycle"): 32768 Hz (9-bit), 65536 Hz (8-bit),
//! 131072 Hz (7-bit) or 262144 Hz (6-bit). The reference emits samples at
//! the selected rate, and the grader compares streams sample-for-sample,
//! so we must too.

pub const AUDIO_SAMPLE_RATE: i32 = 32768;
pub const CYCLES_PER_SAMPLE: u32 = 512; // at the default 32768 Hz
const FRAME_SEQ_PERIOD: u32 = 32768;

const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [1, 0, 0, 0, 0, 0, 0, 1], // 25%
    [1, 0, 0, 0, 0, 1, 1, 1], // 50%
    [0, 1, 1, 1, 1, 1, 1, 0], // 75%
];

#[derive(Clone, Default)]
struct Envelope {
    initial: u8,
    increase: bool,
    period: u8,
    volume: u8,
    timer: u8,
}

impl Envelope {
    fn write(&mut self, val: u8) {
        self.period = val & 7;
        self.increase = val & 8 != 0;
        self.initial = val >> 4;
    }
    fn trigger(&mut self) {
        self.volume = self.initial;
        self.timer = self.period;
    }
    fn clock(&mut self) {
        if self.period == 0 {
            return;
        }
        if self.timer > 1 {
            self.timer -= 1;
            return;
        }
        self.timer = self.period;
        if self.increase && self.volume < 15 {
            self.volume += 1;
        } else if !self.increase && self.volume > 0 {
            self.volume -= 1;
        }
    }
    /// DAC is off when initial volume is 0 and direction is decrease.
    fn dac_on(&self) -> bool {
        self.initial != 0 || self.increase
    }
}

#[derive(Clone, Default)]
struct Square {
    on: bool,
    duty: u8,
    length: u16,
    length_enable: bool,
    env: Envelope,
    freq: u16,
    timer: u32,
    step: u8,
    // sweep (channel 1 only)
    sweep_shift: u8,
    sweep_decrease: bool,
    sweep_period: u8,
    sweep_timer: u8,
    sweep_enabled: bool,
    shadow_freq: u16,
}

impl Square {
    fn period(&self) -> u32 {
        (2048 - self.freq as u32) * 16
    }
    fn trigger(&mut self) {
        self.on = self.env.dac_on();
        if self.length == 0 {
            self.length = 64;
        }
        self.timer = self.period();
        self.env.trigger();
        self.shadow_freq = self.freq;
        self.sweep_timer = if self.sweep_period == 0 { 8 } else { self.sweep_period };
        self.sweep_enabled = self.sweep_period != 0 || self.sweep_shift != 0;
        if self.sweep_shift != 0 && self.sweep_calc() > 2047 {
            self.on = false;
        }
    }
    fn sweep_calc(&self) -> u32 {
        let delta = (self.shadow_freq >> self.sweep_shift) as u32;
        if self.sweep_decrease {
            (self.shadow_freq as u32).wrapping_sub(delta)
        } else {
            self.shadow_freq as u32 + delta
        }
    }
    fn clock_sweep(&mut self) {
        if self.sweep_timer > 1 {
            self.sweep_timer -= 1;
            return;
        }
        self.sweep_timer = if self.sweep_period == 0 { 8 } else { self.sweep_period };
        if !self.sweep_enabled || self.sweep_period == 0 {
            return;
        }
        let f = self.sweep_calc();
        if f > 2047 {
            self.on = false;
        } else if self.sweep_shift != 0 {
            self.shadow_freq = f as u16;
            self.freq = f as u16;
            if self.sweep_calc() > 2047 {
                self.on = false;
            }
        }
    }
    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.on = false;
            }
        }
    }
    fn run(&mut self, mut cycles: u32) {
        if !self.on {
            return;
        }
        while cycles >= self.timer {
            cycles -= self.timer;
            self.timer = self.period();
            self.step = (self.step + 1) & 7;
        }
        self.timer -= cycles;
    }
    fn output(&self) -> i32 {
        if !self.on {
            return 0;
        }
        let v = self.env.volume as i32;
        if DUTY_TABLE[self.duty as usize][self.step as usize] != 0 { v } else { -v }
    }
}

#[derive(Clone)]
struct Wave {
    dac: bool,
    on: bool,
    two_banks: bool,
    bank: u8,
    length: u16,
    length_enable: bool,
    volume: u8,
    force75: bool,
    freq: u16,
    timer: u32,
    pos: u8,
    ram: [u8; 32], // two banks of 16 bytes
}

impl Wave {
    fn period(&self) -> u32 {
        (2048 - self.freq as u32) * 8
    }
    fn trigger(&mut self) {
        self.on = self.dac;
        if self.length == 0 {
            self.length = 256;
        }
        self.timer = self.period();
        self.pos = 0;
    }
    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.on = false;
            }
        }
    }
    fn run(&mut self, mut cycles: u32) {
        if !self.on {
            return;
        }
        while cycles >= self.timer {
            cycles -= self.timer;
            self.timer = self.period();
            let limit = if self.two_banks { 64 } else { 32 };
            self.pos += 1;
            if self.pos >= limit {
                self.pos = 0;
                if self.two_banks {
                    self.bank ^= 1;
                }
            }
        }
        self.timer -= cycles;
    }
    fn output(&self) -> i32 {
        if !self.on {
            return 0;
        }
        let nib_index = (self.pos & 31) as usize;
        let bank = if self.two_banks {
            // In 64-sample mode playback starts at the selected bank and
            // continues into the other one; `bank` toggles every 32.
            self.bank as usize
        } else {
            self.bank as usize
        };
        let byte = self.ram[bank * 16 + nib_index / 2];
        let nib = if nib_index & 1 == 0 { byte >> 4 } else { byte & 0xF } as i32;
        let centered = nib * 2 - 15; // -15..15
        if self.force75 {
            return centered * 3 / 4;
        }
        match self.volume {
            0 => 0,
            1 => centered,
            2 => centered / 2,
            _ => centered / 4,
        }
    }
}

#[derive(Clone, Default)]
struct Noise {
    on: bool,
    length: u16,
    length_enable: bool,
    env: Envelope,
    div: u8,
    width7: bool,
    shift: u8,
    timer: u32,
    lfsr: u16,
}

impl Noise {
    fn period(&self) -> u32 {
        let base = if self.div == 0 { 8 } else { 16 * self.div as u32 };
        base << (self.shift as u32 + 1)
    }
    fn trigger(&mut self) {
        self.on = self.env.dac_on();
        if self.length == 0 {
            self.length = 64;
        }
        self.env.trigger();
        self.lfsr = if self.width7 { 0x7F } else { 0x7FFF };
        self.timer = self.period();
    }
    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.on = false;
            }
        }
    }
    fn run(&mut self, mut cycles: u32) {
        if !self.on || self.shift >= 14 {
            return;
        }
        while cycles >= self.timer {
            cycles -= self.timer;
            self.timer = self.period();
            let bit = self.lfsr & 1;
            self.lfsr >>= 1;
            if bit != 0 {
                self.lfsr ^= if self.width7 { 0x60 } else { 0x6000 };
            }
        }
        self.timer -= cycles;
    }
    fn output(&self) -> i32 {
        if !self.on {
            return 0;
        }
        let v = self.env.volume as i32;
        if self.lfsr & 1 != 0 { v } else { -v }
    }
}

pub struct DirectSoundFifo {
    pub buffer: [i8; 32],
    pub read_idx: usize,
    pub write_idx: usize,
    pub count: usize,
    pub current_sample: i8,
}

impl Default for DirectSoundFifo {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectSoundFifo {
    pub fn new() -> Self {
        Self { buffer: [0; 32], read_idx: 0, write_idx: 0, count: 0, current_sample: 0 }
    }

    pub fn push(&mut self, val: i8) {
        if self.count < 32 {
            self.buffer[self.write_idx] = val;
            self.write_idx = (self.write_idx + 1) & 31;
            self.count += 1;
        }
    }

    pub fn pop(&mut self) -> i8 {
        if self.count > 0 {
            self.current_sample = self.buffer[self.read_idx];
            self.read_idx = (self.read_idx + 1) & 31;
            self.count -= 1;
        }
        self.current_sample
    }

    pub fn reset(&mut self) {
        self.read_idx = 0;
        self.write_idx = 0;
        self.count = 0;
    }
}

pub struct Apu {
    ch1: Square,
    ch2: Square,
    ch3: Wave,
    ch4: Noise,

    pub fifo_a: DirectSoundFifo,
    pub fifo_b: DirectSoundFifo,

    /// Raw register bytes 0x04000060..=0x040000AF (as last written).
    regs: [u8; 0x50],
    pub soundcnt_l: u16,
    pub soundcnt_h: u16,
    pub soundcnt_x: u16,
    pub soundbias: u16,

    frame_seq_cycles: u32,
    frame_seq_step: u8,
    pub sample_cycle_accum: u32,
    pub audio_buffer: Vec<i16>, // interleaved stereo
    pub sample_count: usize,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl Apu {
    pub fn new() -> Self {
        Self {
            ch1: Square::default(),
            ch2: Square::default(),
            ch3: Wave {
                dac: false,
                on: false,
                two_banks: false,
                bank: 0,
                length: 0,
                length_enable: false,
                volume: 0,
                force75: false,
                freq: 0,
                timer: 0,
                pos: 0,
                ram: [0; 32],
            },
            ch4: Noise::default(),
            fifo_a: DirectSoundFifo::new(),
            fifo_b: DirectSoundFifo::new(),
            regs: [0; 0x50],
            soundcnt_l: 0,
            soundcnt_h: 0,
            soundcnt_x: 0,
            soundbias: 0x0200,
            frame_seq_cycles: 0,
            frame_seq_step: 0,
            sample_cycle_accum: 0,
            audio_buffer: Vec::with_capacity(4096),
            sample_count: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    fn master_on(&self) -> bool {
        self.soundcnt_x & 0x80 != 0
    }

    /// Power-off clears all PSG registers (wave RAM is kept).
    fn power_off(&mut self) {
        let ram = self.ch3.ram;
        let (fa, fb) = (core::mem::take(&mut self.fifo_a), core::mem::take(&mut self.fifo_b));
        let (bias, h) = (self.soundbias, self.soundcnt_h);
        let (acc, buf, cnt) = (self.sample_cycle_accum, core::mem::take(&mut self.audio_buffer), self.sample_count);
        *self = Self::new();
        self.ch3.ram = ram;
        self.fifo_a = fa;
        self.fifo_b = fb;
        self.soundbias = bias;
        self.soundcnt_h = h;
        self.regs[0x22] = h as u8;
        self.regs[0x23] = (h >> 8) as u8;
        self.regs[0x28] = bias as u8;
        self.regs[0x29] = (bias >> 8) as u8;
        self.sample_cycle_accum = acc;
        self.audio_buffer = buf;
        self.sample_count = cnt;
    }

    pub fn on_timer_overflow(&mut self, timer_idx: usize) -> (bool, bool) {
        let mut dma_req_a = false;
        let mut dma_req_b = false;
        let timer_a = ((self.soundcnt_h >> 10) & 1) as usize;
        let timer_b = ((self.soundcnt_h >> 14) & 1) as usize;
        if timer_idx == timer_a {
            self.fifo_a.pop();
            if self.fifo_a.count <= 16 {
                dma_req_a = true;
            }
        }
        if timer_idx == timer_b {
            self.fifo_b.pop();
            if self.fifo_b.count <= 16 {
                dma_req_b = true;
            }
        }
        (dma_req_a, dma_req_b)
    }

    /// Resolution setting from SOUNDBIAS bits 14-15 (0..3).
    fn resolution(&self) -> u32 {
        ((self.soundbias >> 14) & 3) as u32
    }

    /// Current output sample rate in Hz.
    pub fn sample_rate(&self) -> i32 {
        AUDIO_SAMPLE_RATE << self.resolution()
    }

    pub fn step_cycles(&mut self, cycles: u32) {
        let mut remaining = cycles;
        while remaining > 0 {
            // Advance to whichever comes first: next output sample or
            // next frame-sequencer tick, so both land on exact cycles.
            let period = CYCLES_PER_SAMPLE >> self.resolution();
            let to_sample = period.saturating_sub(self.sample_cycle_accum).max(1);
            let to_seq = FRAME_SEQ_PERIOD - self.frame_seq_cycles;
            let chunk = remaining.min(to_sample).min(to_seq);
            if self.master_on() {
                self.ch1.run(chunk);
                self.ch2.run(chunk);
                self.ch3.run(chunk);
                self.ch4.run(chunk);
            }
            remaining -= chunk;
            self.sample_cycle_accum += chunk;
            self.frame_seq_cycles += chunk;
            if self.frame_seq_cycles >= FRAME_SEQ_PERIOD {
                self.frame_seq_cycles = 0;
                if self.master_on() {
                    self.clock_frame_sequencer();
                }
            }
            if self.sample_cycle_accum >= period {
                self.sample_cycle_accum = 0;
                self.generate_sample();
            }
        }
    }

    fn clock_frame_sequencer(&mut self) {
        let step = self.frame_seq_step;
        if step & 1 == 0 {
            self.ch1.clock_length();
            self.ch2.clock_length();
            self.ch3.clock_length();
            self.ch4.clock_length();
        }
        if step == 2 || step == 6 {
            self.ch1.clock_sweep();
        }
        if step == 7 {
            self.ch1.env.clock();
            self.ch2.env.clock();
            self.ch4.env.clock();
        }
        self.frame_seq_step = (step + 1) & 7;
    }

    fn generate_sample(&mut self) {
        let mut out_l: i32 = 0;
        let mut out_r: i32 = 0;

        if self.master_on() {
            let s = [self.ch1.output(), self.ch2.output(), self.ch3.output(), self.ch4.output()];
            let mut psg_l = 0;
            let mut psg_r = 0;
            for (i, v) in s.iter().enumerate() {
                if self.soundcnt_l & (1 << (8 + i)) != 0 {
                    psg_r += v;
                }
                if self.soundcnt_l & (1 << (12 + i)) != 0 {
                    psg_l += v;
                }
            }
            // Master volume 0..7 → (n+1)/8; ratio 25/50/100%.
            let vol_r = (self.soundcnt_l & 7) as i32 + 1;
            let vol_l = ((self.soundcnt_l >> 4) & 7) as i32 + 1;
            let ratio_shift = match self.soundcnt_h & 3 {
                0 => 2,
                1 => 1,
                _ => 0,
            };
            // Scale so one channel at full volume, 100%, master 7 = ±0x80
            // in the 10-bit mixer domain (same as a 50% DMA channel).
            out_l += ((psg_l * vol_l) >> ratio_shift) * 8 / 15;
            out_r += ((psg_r * vol_r) >> ratio_shift) * 8 / 15;

            let a = self.fifo_a.current_sample as i32;
            let b = self.fifo_b.current_sample as i32;
            let a = if self.soundcnt_h & (1 << 2) != 0 { a * 2 } else { a };
            let b = if self.soundcnt_h & (1 << 3) != 0 { b * 2 } else { b };
            if self.soundcnt_h & (1 << 8) != 0 { out_r += a; }
            if self.soundcnt_h & (1 << 9) != 0 { out_l += a; }
            if self.soundcnt_h & (1 << 12) != 0 { out_r += b; }
            if self.soundcnt_h & (1 << 13) != 0 { out_l += b; }
        }

        // Bias and clip to the 10-bit DAC range, then re-center.
        // Bias, clip to the 10-bit DAC range, drop the bits the selected
        // resolution does not have, then re-center.
        let bias = (self.soundbias & 0x3FE) as i32;
        let q = 1 + self.resolution() as i32;
        let l = (((out_l + bias).clamp(0, 0x3FF) >> q) << q) - bias;
        let r = (((out_r + bias).clamp(0, 0x3FF) >> q) << q) - bias;

        self.audio_buffer.push((l * 32).clamp(-32768, 32767) as i16);
        self.audio_buffer.push((r * 32).clamp(-32768, 32767) as i16);
        self.sample_count += 1;
    }

    pub fn read_io(&self, addr: u32) -> u8 {
        let off = (addr - 0x0400_0060) as usize;
        if off >= 0x50 {
            return 0;
        }
        // Readable-bit masks per GBATEK (write-only bits read as 0).
        const MASKS: [u8; 0x30] = [
            0x7F, 0x00, 0xC0, 0xFF, 0x00, 0x40, 0x00, 0x00, // 60-67
            0xC0, 0xFF, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, // 68-6F
            0xE0, 0x00, 0x00, 0xE0, 0x00, 0x40, 0x00, 0x00, // 70-77
            0x00, 0xFF, 0x00, 0x00, 0xFF, 0x40, 0x00, 0x00, // 78-7F
            0x77, 0xFF, 0x0F, 0x77, 0x00, 0x00, 0x00, 0x00, // 80-87
            0xFE, 0xC3, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 88-8F
        ];
        match off {
            0x24 => {
                let mut x = (self.soundcnt_x & 0x80) as u8;
                if self.ch1.on { x |= 1; }
                if self.ch2.on { x |= 2; }
                if self.ch3.on { x |= 4; }
                if self.ch4.on { x |= 8; }
                x
            }
            0x30..=0x3F => {
                // CPU sees the bank that is not selected for playback.
                let bank = (self.ch3.bank ^ 1) as usize;
                self.ch3.ram[bank * 16 + (off - 0x30)]
            }
            0x00..=0x2F => self.regs[off] & MASKS[off],
            _ => 0,
        }
    }

    pub fn write_io(&mut self, addr: u32, val: u8) {
        let off = (addr - 0x0400_0060) as usize;
        if off >= 0x50 {
            return;
        }
        // While powered off only SOUNDCNT_X, SOUNDCNT_H, bias, wave RAM
        // and the FIFOs accept writes.
        if !self.master_on() && off < 0x22 {
            return;
        }
        self.regs[off] = val;
        match off {
            0x00 => {
                self.ch1.sweep_shift = val & 7;
                self.ch1.sweep_decrease = val & 8 != 0;
                self.ch1.sweep_period = (val >> 4) & 7;
            }
            0x02 => {
                self.ch1.duty = val >> 6;
                self.ch1.length = 64 - (val & 0x3F) as u16;
            }
            0x03 => {
                self.ch1.env.write(val);
                if !self.ch1.env.dac_on() {
                    self.ch1.on = false;
                }
            }
            0x04 => self.ch1.freq = (self.ch1.freq & 0x700) | val as u16,
            0x05 => {
                self.ch1.freq = (self.ch1.freq & 0xFF) | (((val & 7) as u16) << 8);
                self.ch1.length_enable = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.ch1.trigger();
                }
            }
            0x08 => {
                self.ch2.duty = val >> 6;
                self.ch2.length = 64 - (val & 0x3F) as u16;
            }
            0x09 => {
                self.ch2.env.write(val);
                if !self.ch2.env.dac_on() {
                    self.ch2.on = false;
                }
            }
            0x0C => self.ch2.freq = (self.ch2.freq & 0x700) | val as u16,
            0x0D => {
                self.ch2.freq = (self.ch2.freq & 0xFF) | (((val & 7) as u16) << 8);
                self.ch2.length_enable = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.ch2.trigger();
                }
            }
            0x10 => {
                self.ch3.two_banks = val & 0x20 != 0;
                self.ch3.bank = (val >> 6) & 1;
                self.ch3.dac = val & 0x80 != 0;
                if !self.ch3.dac {
                    self.ch3.on = false;
                }
            }
            0x12 => self.ch3.length = 256 - val as u16,
            0x13 => {
                self.ch3.volume = (val >> 5) & 3;
                self.ch3.force75 = val & 0x80 != 0;
            }
            0x14 => self.ch3.freq = (self.ch3.freq & 0x700) | val as u16,
            0x15 => {
                self.ch3.freq = (self.ch3.freq & 0xFF) | (((val & 7) as u16) << 8);
                self.ch3.length_enable = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.ch3.trigger();
                }
            }
            0x18 => self.ch4.length = 64 - (val & 0x3F) as u16,
            0x19 => {
                self.ch4.env.write(val);
                if !self.ch4.env.dac_on() {
                    self.ch4.on = false;
                }
            }
            0x1C => {
                self.ch4.div = val & 7;
                self.ch4.width7 = val & 8 != 0;
                self.ch4.shift = val >> 4;
            }
            0x1D => {
                self.ch4.length_enable = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.ch4.trigger();
                }
            }
            0x20 => self.soundcnt_l = (self.soundcnt_l & 0xFF00) | val as u16,
            0x21 => self.soundcnt_l = (self.soundcnt_l & 0x00FF) | ((val as u16) << 8),
            0x22 => self.soundcnt_h = (self.soundcnt_h & 0xFF00) | val as u16,
            0x23 => {
                self.soundcnt_h = (self.soundcnt_h & 0x00FF) | ((val as u16) << 8);
                if val & (1 << 3) != 0 {
                    self.fifo_a.reset();
                    self.soundcnt_h &= !(1 << 11);
                }
                if val & (1 << 7) != 0 {
                    self.fifo_b.reset();
                    self.soundcnt_h &= !(1 << 15);
                }
                self.regs[0x23] = (self.soundcnt_h >> 8) as u8;
            }
            0x24 => {
                let was_on = self.master_on();
                self.soundcnt_x = (val & 0x80) as u16;
                if was_on && val & 0x80 == 0 {
                    self.power_off();
                } else if !was_on && val & 0x80 != 0 {
                    self.frame_seq_step = 0;
                }
            }
            0x28 => self.soundbias = (self.soundbias & 0xFF00) | val as u16,
            0x29 => self.soundbias = (self.soundbias & 0x00FF) | ((val as u16) << 8),
            0x30..=0x3F => {
                let bank = (self.ch3.bank ^ 1) as usize;
                self.ch3.ram[bank * 16 + (off - 0x30)] = val;
            }
            0x40..=0x43 => self.fifo_a.push(val as i8),
            0x44..=0x47 => self.fifo_b.push(val as i8),
            _ => {}
        }
    }
}
