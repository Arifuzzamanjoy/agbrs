//! Encapsulated GBA State Machine.

use crate::cpu::Cpu;
use crate::bus::Bus;
use crate::apu::AUDIO_SAMPLE_RATE;

pub struct Gba {
    pub cpu: Cpu,
    pub bus: Bus,
    pub rom_buffer: Vec<u8>,
    pub audio_buffer: Box<[i16; 8192]>,
    pub audio_sample_count: usize,
}

impl Default for Gba {
    fn default() -> Self {
        Self::new()
    }
}

impl Gba {
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            bus: Bus::new(),
            rom_buffer: vec![0u8; 32 * 1024 * 1024],
            audio_buffer: Box::new([0i16; 8192]),
            audio_sample_count: 0,
        }
    }

    pub fn load_rom(&mut self, len: usize) -> bool {
        let len = len.min(self.rom_buffer.len());
        self.bus.rom.clear();
        self.bus.rom.extend_from_slice(&self.rom_buffer[..len]);
        self.reset()
    }

    pub fn reset(&mut self) -> bool {
        self.bus.reset();
        let bios_stub = include_bytes!("../spec/gba_bios_stub.bin");
        self.bus.bios[..bios_stub.len()].copy_from_slice(bios_stub);
        self.cpu.reset(&mut self.bus);
        self.audio_sample_count = 0;
        true
    }

    pub fn set_keys(&mut self, keys: u32) {
        // GBA KEYINPUT register bits 0..9 are active-low (0 = pressed)
        let active_low = (!(keys as u16)) & 0x03FF;
        self.bus.io.keyinput = active_low;
    }

    pub fn run_frame(&mut self) {
        const CYCLES_PER_FRAME: u32 = 280_896;
        let mut total_cycles = 0u32;

        while total_cycles < CYCLES_PER_FRAME {
            let step = self.cpu.step(&mut self.bus);
            total_cycles += step;

            // 1. Timers
            let (t0_ovf, t1_ovf) = self.bus.timers.step(step, &mut self.bus.io.if_);
            if t0_ovf || t1_ovf {
                let (dma_a0, dma_b0) = if t0_ovf { self.bus.apu.on_timer_overflow(0) } else { (false, false) };
                let (dma_a1, dma_b1) = if t1_ovf { self.bus.apu.on_timer_overflow(1) } else { (false, false) };
                if dma_a0 || dma_a1 || dma_b0 || dma_b1 {
                    self.bus.trigger_dma_fifo(dma_a0 || dma_a1, dma_b0 || dma_b1);
                }
            }

            // 2. PPU
            let (vblank_trig, hblank_trig) = self.bus.ppu.step_cycles(
                step,
                &mut self.bus.io.if_,
                &self.bus.palette[..],
                &self.bus.vram[..],
                &self.bus.oam[..],
            );
            if vblank_trig {
                self.bus.trigger_dma(crate::dma::DmaTrigger::VBlank);
            }
            if hblank_trig {
                self.bus.trigger_dma(crate::dma::DmaTrigger::HBlank);
            }

            // 3. APU
            self.bus.apu.step_cycles(step);
        }

        // Copy generated audio samples to audio_buffer
        let count = self.bus.apu.sample_count;
        let available_pairs = (self.audio_buffer.len() / 2).saturating_sub(self.audio_sample_count);
        let to_copy = count.min(available_pairs);
        let src = &self.bus.apu.audio_buffer[..to_copy * 2];
        let dst_start = self.audio_sample_count * 2;
        self.audio_buffer[dst_start..dst_start + to_copy * 2].copy_from_slice(src);
        self.audio_sample_count += to_copy;
        self.bus.apu.audio_buffer.clear();
        self.bus.apu.sample_count = 0;
    }

    #[inline(always)]
    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.bus.ppu.framebuffer.as_ptr()
    }

    #[inline(always)]
    pub fn audio_buffer_ptr(&self) -> *const i16 {
        self.audio_buffer.as_ptr()
    }

    pub fn drain_audio_samples(&mut self) -> i32 {
        let count = self.audio_sample_count as i32;
        self.audio_sample_count = 0;
        count
    }

    #[inline(always)]
    pub fn audio_rate(&self) -> i32 {
        AUDIO_SAMPLE_RATE
    }
}
