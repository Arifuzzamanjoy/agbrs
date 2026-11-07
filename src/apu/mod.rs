//! Game Boy Advance Audio Processing Unit (APU).

pub const AUDIO_SAMPLE_RATE: i32 = 32768;
pub const CYCLES_PER_SAMPLE: u32 = 512; // 16777216 / 32768 = 512

pub struct SquareChannel {
    pub enabled: bool,
    pub duty: u8,         // 0=12.5%, 1=25%, 2=50%, 3=75%
    pub length: u8,
    pub length_flag: bool,
    pub env_initial: u8,
    pub env_inc: bool,
    pub env_step: u8,
    pub cur_volume: u8,
    pub env_timer: u8,
    pub frequency: u16,
    pub freq_timer: u32,
    pub duty_step: u8,

    // Sweep (CH1 only)
    pub sweep_shift: u8,
    pub sweep_inc: bool,
    pub sweep_time: u8,
    pub sweep_timer: u8,
}

impl SquareChannel {
    pub fn new() -> Self {
        Self {
            enabled: false,
            duty: 2,
            length: 0,
            length_flag: false,
            env_initial: 0,
            env_inc: false,
            env_step: 0,
            cur_volume: 0,
            env_timer: 0,
            frequency: 0,
            freq_timer: 0,
            duty_step: 0,
            sweep_shift: 0,
            sweep_inc: false,
            sweep_time: 0,
            sweep_timer: 0,
        }
    }

    pub fn trigger(&mut self) {
        self.enabled = true;
        self.cur_volume = self.env_initial;
        self.env_timer = self.env_step;
        self.freq_timer = (2048 - self.frequency as u32) * 4;
    }

    pub fn step_timer(&mut self, cycles: u32) {
        if !self.enabled { return; }

        if cycles >= self.freq_timer {
            self.duty_step = (self.duty_step + 1) & 7;
            self.freq_timer = (2048 - self.frequency as u32) * 4;
        } else {
            self.freq_timer -= cycles;
        }
    }

    pub fn sample(&self) -> i16 {
        if !self.enabled || self.cur_volume == 0 {
            return 0;
        }

        const DUTY_TABLE: [[u8; 8]; 4] = [
            [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
            [1, 0, 0, 0, 0, 0, 0, 1], // 25%
            [1, 0, 0, 0, 0, 1, 1, 1], // 50%
            [0, 1, 1, 1, 1, 1, 1, 0], // 75%
        ];

        let high = DUTY_TABLE[self.duty as usize][self.duty_step as usize] != 0;
        if high {
            (self.cur_volume as i16) * 128
        } else {
            -((self.cur_volume as i16) * 128)
        }
    }
}

pub struct WaveChannel {
    pub enabled: bool,
    pub bank_mode: bool,
    pub cur_bank: u8,
    pub length: u8,
    pub length_flag: bool,
    pub volume_shift: u8, // 0=mute, 1=100%, 2=50%, 3=25%
    pub frequency: u16,
    pub freq_timer: u32,
    pub sample_index: u8,
    pub wave_ram: [u8; 32], // 32 4-bit samples
}

impl WaveChannel {
    pub fn new() -> Self {
        Self {
            enabled: false,
            bank_mode: false,
            cur_bank: 0,
            length: 0,
            length_flag: false,
            volume_shift: 0,
            frequency: 0,
            freq_timer: 0,
            sample_index: 0,
            wave_ram: [0; 32],
        }
    }

    pub fn trigger(&mut self) {
        self.enabled = true;
        self.sample_index = 0;
        self.freq_timer = (2048 - self.frequency as u32) * 2;
    }

    pub fn step_timer(&mut self, cycles: u32) {
        if !self.enabled { return; }

        if cycles >= self.freq_timer {
            self.sample_index = (self.sample_index + 1) & 31;
            self.freq_timer = (2048 - self.frequency as u32) * 2;
        } else {
            self.freq_timer -= cycles;
        }
    }

    pub fn sample(&self) -> i16 {
        if !self.enabled || self.volume_shift == 0 {
            return 0;
        }

        let raw = self.wave_ram[self.sample_index as usize];
        let shifted = match self.volume_shift {
            1 => raw,
            2 => raw >> 1,
            3 => raw >> 2,
            _ => (raw * 3) / 4,
        };

        ((shifted as i16) - 8) * 128
    }
}

pub struct NoiseChannel {
    pub enabled: bool,
    pub length: u8,
    pub length_flag: bool,
    pub env_initial: u8,
    pub env_inc: bool,
    pub env_step: u8,
    pub cur_volume: u8,
    pub env_timer: u8,
    pub lfsr: u16,
    pub narrow_mode: bool,
    pub div_ratio: u8,
    pub freq_shift: u8,
    pub freq_timer: u32,
}

impl NoiseChannel {
    pub fn new() -> Self {
        Self {
            enabled: false,
            length: 0,
            length_flag: false,
            env_initial: 0,
            env_inc: false,
            env_step: 0,
            cur_volume: 0,
            env_timer: 0,
            lfsr: 0x7FFF,
            narrow_mode: false,
            div_ratio: 0,
            freq_shift: 0,
            freq_timer: 0,
        }
    }

    pub fn trigger(&mut self) {
        self.enabled = true;
        self.cur_volume = self.env_initial;
        self.env_timer = self.env_step;
        self.lfsr = 0x7FFF;
        self.freq_timer = self.calc_period();
    }

    fn calc_period(&self) -> u32 {
        let r = match self.div_ratio {
            0 => 8,
            _ => (self.div_ratio as u32) * 16,
        };
        r << (self.freq_shift as u32)
    }

    pub fn step_timer(&mut self, cycles: u32) {
        if !self.enabled { return; }

        if cycles >= self.freq_timer {
            let carry = (self.lfsr & 1) ^ ((self.lfsr >> 1) & 1);
            self.lfsr = (self.lfsr >> 1) | (carry << 14);
            if self.narrow_mode {
                self.lfsr = (self.lfsr & !(1 << 6)) | (carry << 6);
            }
            self.freq_timer = self.calc_period();
        } else {
            self.freq_timer -= cycles;
        }
    }

    pub fn sample(&self) -> i16 {
        if !self.enabled || self.cur_volume == 0 {
            return 0;
        }

        if (self.lfsr & 1) == 0 {
            (self.cur_volume as i16) * 128
        } else {
            -((self.cur_volume as i16) * 128)
        }
    }
}

pub struct DirectSoundFifo {
    pub buffer: [i8; 32],
    pub read_idx: usize,
    pub write_idx: usize,
    pub count: usize,
    pub current_sample: i8,
}

impl DirectSoundFifo {
    pub fn new() -> Self {
        Self {
            buffer: [0; 32],
            read_idx: 0,
            write_idx: 0,
            count: 0,
            current_sample: 0,
        }
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
        self.current_sample = 0;
    }
}

pub struct Apu {
    pub ch1: SquareChannel,
    pub ch2: SquareChannel,
    pub ch3: WaveChannel,
    pub ch4: NoiseChannel,

    pub fifo_a: DirectSoundFifo,
    pub fifo_b: DirectSoundFifo,

    pub soundcnt_l: u16,
    pub soundcnt_h: u16,
    pub soundcnt_x: u16,
    pub soundbias: u16,

    pub sample_cycle_accum: u32,
    pub audio_buffer: Vec<i16>, // Interleaved stereo L, R, L, R...
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
            ch1: SquareChannel::new(),
            ch2: SquareChannel::new(),
            ch3: WaveChannel::new(),
            ch4: NoiseChannel::new(),
            fifo_a: DirectSoundFifo::new(),
            fifo_b: DirectSoundFifo::new(),
            soundcnt_l: 0,
            soundcnt_h: 0,
            soundcnt_x: 0x80, // Master enable on
            soundbias: 0x0200,
            sample_cycle_accum: 0,
            audio_buffer: Vec::with_capacity(4096),
            sample_count: 0,
        }
    }

    pub fn reset(&mut self) {
        self.ch1 = SquareChannel::new();
        self.ch2 = SquareChannel::new();
        self.ch3 = WaveChannel::new();
        self.ch4 = NoiseChannel::new();
        self.fifo_a.reset();
        self.fifo_b.reset();
        self.soundcnt_l = 0;
        self.soundcnt_h = 0;
        self.soundcnt_x = 0x80;
        self.soundbias = 0x0200;
        self.sample_cycle_accum = 0;
        self.audio_buffer.clear();
        self.sample_count = 0;
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

    pub fn step_cycles(&mut self, cycles: u32) {
        self.ch1.step_timer(cycles);
        self.ch2.step_timer(cycles);
        self.ch3.step_timer(cycles);
        self.ch4.step_timer(cycles);

        self.sample_cycle_accum += cycles;
        while self.sample_cycle_accum >= CYCLES_PER_SAMPLE {
            self.sample_cycle_accum -= CYCLES_PER_SAMPLE;
            self.generate_sample();
        }
    }

    fn generate_sample(&mut self) {
        if (self.soundcnt_x & (1 << 7)) == 0 {
            self.audio_buffer.push(0);
            self.audio_buffer.push(0);
            self.sample_count += 1;
            return;
        }

        // Sample PSG channels
        let s1 = self.ch1.sample();
        let s2 = self.ch2.sample();
        let s3 = self.ch3.sample();
        let s4 = self.ch4.sample();

        let mut psg_l: i32 = 0;
        let mut psg_r: i32 = 0;

        if self.soundcnt_l & (1 << 8) != 0 { psg_r += s1 as i32; }
        if self.soundcnt_l & (1 << 9) != 0 { psg_r += s2 as i32; }
        if self.soundcnt_l & (1 << 10) != 0 { psg_r += s3 as i32; }
        if self.soundcnt_l & (1 << 11) != 0 { psg_r += s4 as i32; }

        if self.soundcnt_l & (1 << 12) != 0 { psg_l += s1 as i32; }
        if self.soundcnt_l & (1 << 13) != 0 { psg_l += s2 as i32; }
        if self.soundcnt_l & (1 << 14) != 0 { psg_l += s3 as i32; }
        if self.soundcnt_l & (1 << 15) != 0 { psg_l += s4 as i32; }

        let psg_vol_r = (self.soundcnt_l & 7) as i32;
        let psg_vol_l = ((self.soundcnt_l >> 4) & 7) as i32;

        let mut out_l = (psg_l * psg_vol_l) / 8;
        let mut out_r = (psg_r * psg_vol_r) / 8;

        // Sample DirectSound FIFO A & B
        let dsa_vol = if self.soundcnt_h & (1 << 2) != 0 { 100 } else { 50 };
        let dsb_vol = if self.soundcnt_h & (1 << 3) != 0 { 100 } else { 50 };

        let sample_a = ((self.fifo_a.current_sample as i32) * 512 * dsa_vol) / 100;
        let sample_b = ((self.fifo_b.current_sample as i32) * 512 * dsb_vol) / 100;

        if self.soundcnt_h & (1 << 8) != 0 { out_r += sample_a; }
        if self.soundcnt_h & (1 << 9) != 0 { out_l += sample_a; }
        if self.soundcnt_h & (1 << 12) != 0 { out_r += sample_b; }
        if self.soundcnt_h & (1 << 13) != 0 { out_l += sample_b; }

        // Clamp to 16-bit signed PCM
        let final_l = out_l.clamp(-32768, 32767) as i16;
        let final_r = out_r.clamp(-32768, 32767) as i16;

        self.audio_buffer.push(final_l);
        self.audio_buffer.push(final_r);
        self.sample_count += 1;
    }

    pub fn read_io(&self, addr: u32) -> u8 {
        match addr {
            0x04000080 => (self.soundcnt_l & 0xFF) as u8,
            0x04000081 => ((self.soundcnt_l >> 8) & 0xFF) as u8,
            0x04000082 => (self.soundcnt_h & 0xFF) as u8,
            0x04000083 => ((self.soundcnt_h >> 8) & 0xFF) as u8,
            0x04000084 => {
                let mut x = (self.soundcnt_x & 0x80) as u8;
                if self.ch1.enabled { x |= 1; }
                if self.ch2.enabled { x |= 2; }
                if self.ch3.enabled { x |= 4; }
                if self.ch4.enabled { x |= 8; }
                x
            }
            _ => 0,
        }
    }

    pub fn write_io(&mut self, addr: u32, val: u8) {
        match addr {
            0x04000060 => self.ch1.sweep_shift = val & 7,
            0x04000062 => {
                self.ch1.duty = (val >> 6) & 3;
                self.ch1.length = val & 0x3F;
            }
            0x04000063 => {
                self.ch1.env_step = val & 7;
                self.ch1.env_inc = val & 8 != 0;
                self.ch1.env_initial = val >> 4;
            }
            0x04000064 => {
                self.ch1.frequency = (self.ch1.frequency & 0x0700) | (val as u16);
            }
            0x04000065 => {
                self.ch1.frequency = (self.ch1.frequency & 0x00FF) | (((val & 7) as u16) << 8);
                self.ch1.length_flag = (val & 0x40) != 0;
                if val & 0x80 != 0 {
                    self.ch1.trigger();
                }
            }

            0x04000068 => {
                self.ch2.duty = (val >> 6) & 3;
                self.ch2.length = val & 0x3F;
            }
            0x04000069 => {
                self.ch2.env_step = val & 7;
                self.ch2.env_inc = val & 8 != 0;
                self.ch2.env_initial = val >> 4;
            }
            0x0400006C => {
                self.ch2.frequency = (self.ch2.frequency & 0x0700) | (val as u16);
            }
            0x0400006D => {
                self.ch2.frequency = (self.ch2.frequency & 0x00FF) | (((val & 7) as u16) << 8);
                self.ch2.length_flag = (val & 0x40) != 0;
                if val & 0x80 != 0 {
                    self.ch2.trigger();
                }
            }

            0x04000072 => {
                self.ch3.volume_shift = (val >> 5) & 3;
            }
            0x04000074 => {
                self.ch3.frequency = (self.ch3.frequency & 0x0700) | (val as u16);
            }
            0x04000075 => {
                self.ch3.frequency = (self.ch3.frequency & 0x00FF) | (((val & 7) as u16) << 8);
                self.ch3.length_flag = (val & 0x40) != 0;
                if val & 0x80 != 0 {
                    self.ch3.trigger();
                }
            }

            0x04000078 => self.ch4.length = val & 0x3F,
            0x04000079 => {
                self.ch4.env_step = val & 7;
                self.ch4.env_inc = val & 8 != 0;
                self.ch4.env_initial = val >> 4;
            }
            0x0400007C => {
                self.ch4.div_ratio = val & 7;
                self.ch4.narrow_mode = val & 8 != 0;
                self.ch4.freq_shift = val >> 4;
            }
            0x0400007D => {
                self.ch4.length_flag = (val & 0x40) != 0;
                if val & 0x80 != 0 {
                    self.ch4.trigger();
                }
            }

            0x04000080 => self.soundcnt_l = (self.soundcnt_l & 0xFF00) | (val as u16),
            0x04000081 => self.soundcnt_l = (self.soundcnt_l & 0x00FF) | ((val as u16) << 8),
            0x04000082 => self.soundcnt_h = (self.soundcnt_h & 0xFF00) | (val as u16),
            0x04000083 => {
                self.soundcnt_h = (self.soundcnt_h & 0x00FF) | ((val as u16) << 8);
                if val & (1 << 3) != 0 { self.fifo_a.reset(); }
                if val & (1 << 7) != 0 { self.fifo_b.reset(); }
            }
            0x04000084 => {
                self.soundcnt_x = (self.soundcnt_x & 0x007F) | ((val as u16) & 0x80);
                if (val & 0x80) == 0 {
                    self.reset();
                    self.soundcnt_x = 0;
                }
            }

            0x040000A0..=0x040000A3 => self.fifo_a.push(val as i8),
            0x040000A4..=0x040000A7 => self.fifo_b.push(val as i8),

            0x04000090..=0x0400009F => {
                let off = (addr - 0x04000090) as usize;
                self.ch3.wave_ram[off * 2] = val >> 4;
                self.ch3.wave_ram[off * 2 + 1] = val & 0xF;
            }

            _ => {}
        }
    }
}

