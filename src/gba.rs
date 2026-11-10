//! Encapsulated GBA State Machine.

use crate::cpu::Cpu;
use crate::bus::Bus;

pub struct Gba {
    pub cpu: Cpu,
    pub bus: Bus,
    pub rom_buffer: Vec<u8>,
    /// Interleaved stereo; sized for several frames at the highest rate
    /// (262144 Hz ≈ 4390 pairs/frame).
    pub audio_buffer: Box<[i16; 65536]>,
    pub audio_sample_count: usize,
    /// Debug-only instruction trace (pc, opcode, thumb) for one frame.
    pub trace: Option<Vec<(u32, u32, bool)>>,
    /// Set when the PPU enters VBlank; `run_frame` returns there, the
    /// same frame boundary the reference uses.
    frame_done: bool,
    pending_keys: u32,
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
            audio_buffer: Box::new([0i16; 65536]),
            audio_sample_count: 0,
            trace: None,
            frame_done: false,
            pending_keys: 0,
        }
    }

    pub fn load_rom(&mut self, len: usize) -> bool {
        let len = len.min(self.rom_buffer.len());
        self.bus.rom.clear();
        self.bus.rom.extend_from_slice(&self.rom_buffer[..len]);
        self.reset()
    }

    pub fn reset(&mut self) -> bool {
        // Fresh CPU too: `Cpu::reset` alone keeps HALT/IRQ bookkeeping.
        self.cpu = Cpu::new();
        self.bus.reset();
        let bios_stub = include_bytes!("../spec/gba_bios_stub.bin");
        self.bus.bios[..bios_stub.len()].copy_from_slice(bios_stub);
        self.cpu.reset_skip_bios(&mut self.bus);
        // Cycles of the initial pipeline fill count toward the clock; the
        // reference's first ROM opcode executes at clock 21.
        const BOOT_CLOCK: u32 = 21;
        let fill = self.bus.take_cycles().max(BOOT_CLOCK);
        self.bus.now = fill as u64;
        self.bus.sync_timers();
        self.bus.ppu.scanline_cycles = fill;
        self.audio_sample_count = 0;
        self.frame_done = false;
        self.pending_keys = 0;
        true
    }

    /// Keys are latched: they become visible to the game at the end of
    /// the next `run_frame` (the VBlank boundary), which is when the
    /// reference polls its input provider (verified frame-by-frame:
    /// applying them immediately made every input take effect one frame
    /// early).
    pub fn set_keys(&mut self, keys: u32) {
        self.pending_keys = keys;
    }

    fn latch_keys(&mut self) {
        // KEYINPUT bits 0..9 are active-low (0 = pressed).
        self.bus.io.keyinput = (!(self.pending_keys as u16)) & 0x03FF;
    }

    /// Execute one CPU step (instruction, IRQ entry, or halted
    /// skip-ahead) and advance timers, PPU and APU by the same cycles.
    pub fn step_one(&mut self) -> u32 {
        if let Some(t) = self.trace.as_mut() {
            let thumb = self.cpu.cpsr & 0x20 != 0;
            let pc = self.cpu.r[15].wrapping_sub(if thumb { 4 } else { 8 });
            t.push((pc, self.cpu.pipe[0], thumb));
        }
        let step = self.cpu.step(&mut self.bus);

        // Timers are synced lazily (also on timer I/O access).
        self.bus.now += step as u64;
        self.bus.sync_timers();

        // Feed the PPU in chunks that end on its events, so a long step
        // (e.g. a big DMA) still produces every HBlank/VBlank in order.
        let mut rem = step;
        while rem > 0 {
            let chunk = rem.min(self.bus.ppu.cycles_to_next_event());
            rem -= chunk;
            let if_before = self.bus.io.if_;
            let (vblank_trig, hblank_trig) = self.bus.ppu.step_cycles(
                chunk,
                &mut self.bus.io.if_,
                &self.bus.palette[..],
                &self.bus.vram[..],
                &self.bus.oam[..],
            );
            if self.bus.io.if_ != if_before {
                let chunk_end = self.bus.now as i64 - rem as i64;
                let t = chunk_end - (self.bus.ppu.irq_overshoot as i64).min(chunk as i64);
                self.bus.update_irq_line(t as u64);
            }
            if vblank_trig {
                self.frame_done = true;
                self.bus.trigger_dma(crate::dma::DmaTrigger::VBlank);
            }
            if hblank_trig {
                self.bus.trigger_dma(crate::dma::DmaTrigger::HBlank);
            }
        }

        self.bus.apu.step_cycles(step);
        step
    }

    pub fn run_frame(&mut self) {
        // A frame ends when the PPU enters VBlank (line 160). Cap at two
        // frames' worth of cycles in case the PPU state is ever odd.
        const CYCLES_PER_FRAME: u32 = 280_896;
        let mut total_cycles = 0u32;
        self.frame_done = false;
        while !self.frame_done && total_cycles < 2 * CYCLES_PER_FRAME {
            total_cycles += self.step_one();
        }
        self.latch_keys();

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
        self.bus.apu.sample_rate()
    }
}
