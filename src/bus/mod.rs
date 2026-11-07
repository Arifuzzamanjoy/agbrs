//! GBA Memory Bus, MMIO Dispatch, and Peripheral Integration.

pub mod io;
pub mod waitstates;

use io::IoRegisters;
use waitstates::WaitStates;
use crate::ppu::Ppu;
use crate::apu::Apu;
use crate::dma::{DmaController, DmaTrigger};
use crate::timer::Timers;

const VRAM_UNMAPPED: usize = usize::MAX;
/// Fixed cost of starting a DMA transfer.
pub const DMA_OVERHEAD_CYCLES: u32 = 4;

pub struct Bus {
    pub bios: Box<[u8; 0x4000]>,
    pub ewram: Box<[u8; 0x40000]>,
    pub iwram: Box<[u8; 0x8000]>,
    pub palette: Box<[u8; 0x400]>,
    pub vram: Box<[u8; 0x18000]>,
    pub oam: Box<[u8; 0x400]>,
    pub rom: Vec<u8>,
    pub sram: Box<[u8; 0x10000]>,

    pub io: IoRegisters,
    pub ppu: Ppu,
    pub apu: Apu,
    pub dma: DmaController,
    pub timers: Timers,
    pub ws: WaitStates,
    pub open_bus: u32,

    /// Cycles consumed by bus activity (CPU accesses, DMA) not yet
    /// handed to the scheduler. Drained by the CPU step.
    pub cycles: u32,
    /// Address immediately after the previous bus access; an access at
    /// exactly this address is treated as sequential (S), else non-seq (N).
    next_seq_addr: u32,
    /// Game Pak prefetch buffer model (WAITCNT bit 14).
    pf_active: bool,
    pf_head: u32,
    pf_count: u32,
    pf_progress: u32,
    /// Set by a write to HALTCNT; consumed by the CPU.
    pub halt_request: bool,
    /// Last value written to each I/O halfword, so byte writes to
    /// write-only registers can be merged into a 16-bit write.
    io_shadow: Box<[u16; 0x200]>,
    /// Debug watchpoint (native builds only): log accesses to this word.
    pub watch: Option<u32>,
    /// Last opcode word fetched from BIOS; returned for BIOS reads made
    /// while executing outside the BIOS (read protection).
    bios_latch: u32,
    /// Whether the most recent opcode fetch was from BIOS.
    fetch_in_bios: bool,
    /// Global cycle count at the start of the current CPU step; the
    /// exact time of an access is `now + cycles`.
    pub now: u64,
    timer_sync: u64,
    /// Time at which (IE & IF) != 0 && IME last became true; None while
    /// the IRQ line is low. The CPU takes the IRQ `IRQ_DELAY` cycles later.
    pub irq_since: Option<u64>,
    /// Last value moved by DMA; returned when DMA reads an unreadable
    /// source (BIOS / unmapped).
    dma_latch: u32,
    /// Debug (native only): per-access log of (addr, width, kind, cost).
    pub access_log: Option<Vec<(u32, u32, char, u32)>>,
    /// Set whenever a DMA transfer ran; the CPU executes one more
    /// instruction before it can take an IRQ (lockstep vs reference).
    pub dma_ran: bool,
    /// Debug: times at which the IRQ line rose (bounded log).
    pub irq_rise_log: Vec<u64>,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus {
    pub fn new() -> Self {
        Self {
            bios: Box::new([0; 0x4000]),
            ewram: Box::new([0; 0x40000]),
            iwram: Box::new([0; 0x8000]),
            palette: Box::new([0; 0x400]),
            vram: Box::new([0; 0x18000]),
            oam: Box::new([0; 0x400]),
            rom: Vec::new(),
            sram: Box::new([0; 0x10000]),
            io: IoRegisters::new(),
            ppu: Ppu::new(),
            apu: Apu::new(),
            dma: DmaController::new(),
            timers: Timers::new(),
            ws: WaitStates::new(),
            open_bus: 0,
            cycles: 0,
            next_seq_addr: u32::MAX,
            pf_active: false,
            pf_head: 0,
            pf_count: 0,
            pf_progress: 0,
            halt_request: false,
            io_shadow: Box::new([0; 0x200]),
            watch: None,
            bios_latch: 0,
            fetch_in_bios: true,
            now: 0,
            timer_sync: 0,
            irq_since: None,
            dma_latch: 0,
            access_log: None,
            dma_ran: false,
            irq_rise_log: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.ewram.fill(0);
        self.iwram.fill(0);
        self.palette.fill(0);
        self.vram.fill(0);
        self.oam.fill(0);
        self.sram.fill(0);
        self.io = IoRegisters::new();
        self.ppu.reset();
        self.apu.reset();
        self.dma = DmaController::new();
        self.timers = Timers::new();
        self.ws = WaitStates::new();
        self.open_bus = 0;
        self.cycles = 0;
        self.next_seq_addr = u32::MAX;
        self.pf_active = false;
        self.pf_count = 0;
        self.pf_progress = 0;
        self.halt_request = false;
        self.io_shadow.fill(0);
        self.bios_latch = 0;
        self.fetch_in_bios = true;
        self.now = 0;
        self.timer_sync = 0;
        self.irq_since = None;
    }

    /// Re-evaluate the IRQ line after IE/IF/IME changed at time `t`.
    #[inline]
    pub fn update_irq_line(&mut self, t: u64) {
        if self.io.irq_pending() {
            if self.irq_since.is_none() {
                self.irq_since = Some(t);
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if self.irq_rise_log.len() > 64 {
                        self.irq_rise_log.remove(0);
                    }
                    self.irq_rise_log.push(t);
                }
            }
        } else {
            self.irq_since = None;
        }
    }

    #[inline(always)]
    pub fn access_time(&self) -> u64 {
        self.now + self.cycles as u64
    }

    /// Bring timers up to the current exact cycle, raising overflow
    /// side effects (IRQ, DirectSound FIFO pop + DMA request).
    pub fn sync_timers(&mut self) {
        let t = self.now + self.cycles as u64;
        if t <= self.timer_sync {
            return;
        }
        let mut d = t - self.timer_sync;
        self.timer_sync = t;
        while d > 0 {
            // Step at most up to the next overflow so each one is seen.
            let chunk = d.min(self.timers.cycles_to_overflow() as u64).max(1) as u32;
            d -= chunk as u64;
            let if_before = self.io.if_;
            let (t0, t1) = self.timers.step(chunk, &mut self.io.if_);
            if self.io.if_ != if_before {
                let t_chunk = t - d;
                self.update_irq_line(t_chunk);
            }
            if t0 || t1 {
                let (a0, b0) = if t0 { self.apu.on_timer_overflow(0) } else { (false, false) };
                let (a1, b1) = if t1 { self.apu.on_timer_overflow(1) } else { (false, false) };
                if a0 || a1 || b0 || b1 {
                    self.trigger_dma_fifo(a0 || a1, b0 || b1);
                }
            }
        }
    }

    /// Cycles until the next scheduled hardware event (used to skip
    /// ahead while the CPU is halted).
    pub fn cycles_to_next_event(&self) -> u32 {
        self.ppu.cycles_to_next_event().min(self.timers.cycles_to_overflow()).max(1)
    }

    // ── Timing ───────────────────────────────────────────────────────
    //
    // Every CPU/DMA access is charged its GBATEK cost: 1 + waitstates,
    // where Game Pak waitstates depend on whether the access is
    // sequential to the previous one. ROM code fetches go through a
    // model of the 8-halfword prefetch buffer.

    #[inline(always)]
    fn is_gamepak_rom(addr: u32) -> bool {
        (0x08..=0x0D).contains(&(addr >> 24))
    }

    #[inline(always)]
    fn access_cost(&mut self, addr: u32, width: u32) -> u32 {
        let seq = addr == self.next_seq_addr;
        self.next_seq_addr = addr.wrapping_add(width);
        let mode = if seq { waitstates::SEQUENTIAL } else { 0 }
            | if width == 4 { waitstates::WORD } else { 0 };
        self.ws.get(addr, mode) as u32
    }

    /// Cycles spent while the Game Pak bus is idle let the prefetch
    /// buffer fill.
    #[inline(always)]
    fn prefetch_advance(&mut self, cycles: u32) {
        if !self.pf_active || self.pf_count >= 8 {
            return;
        }
        let s = self.ws.get(self.pf_head | 2, waitstates::SEQUENTIAL) as u32;
        self.pf_progress += cycles;
        while self.pf_progress >= s && self.pf_count < 8 {
            self.pf_progress -= s;
            self.pf_count += 1;
        }
        if self.pf_count >= 8 {
            self.pf_progress = 0;
        }
    }

    #[inline(always)]
    fn log_access(&mut self, addr: u32, width: u32, kind: char, cost: u32) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(l) = self.access_log.as_mut() {
            l.push((addr, width, kind, cost));
        }
        let _ = (addr, width, kind, cost);
    }

    #[inline(always)]
    fn data_access(&mut self, addr: u32, width: u32) {
        // A Game Pak data access that interrupts an in-flight prefetch
        // costs one extra cycle (lockstep vs reference).
        if (addr >> 24) >= 0x08 && self.pf_active && self.pf_progress > 0 && self.pf_count < 8 {
            self.cycles += 1;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(w) = self.watch {
            if (addr & !3) == (w & !3) {
                eprintln!("watch: access {addr:08X} width {width}");
            }
        }
        let c = self.access_cost(addr, width);
        if (addr >> 24) >= 0x08 {
            // Game Pak data access interrupts and discards the prefetch.
            self.pf_active = false;
            self.pf_count = 0;
            self.pf_progress = 0;
        } else {
            self.prefetch_advance(c);
        }
        self.log_access(addr, width, 'D', c);
        self.cycles += c;
    }

    #[inline(always)]
    fn code_access(&mut self, addr: u32, width: u32) {
        if !Self::is_gamepak_rom(addr) {
            let c = self.access_cost(addr, width);
            self.prefetch_advance(c);
            self.log_access(addr, width, 'C', c);
            self.cycles += c;
            return;
        }
        if self.io.waitcnt & 0x4000 != 0 && self.pf_active && addr == self.pf_head {
            // Served from the prefetch buffer: 1 cycle per halfword
            // available, otherwise stall for the in-flight fetch.
            let s = self.ws.get(addr | 2, waitstates::SEQUENTIAL) as u32;
            let mut cost = 0;
            for _ in 0..width / 2 {
                if self.pf_count > 0 {
                    self.pf_count -= 1;
                    cost += 1;
                    // The prefetcher keeps running during this cycle.
                    self.prefetch_advance(1);
                } else {
                    let remaining = s.saturating_sub(self.pf_progress).max(1);
                    cost += remaining;
                    self.pf_progress = 0;
                }
            }
            self.pf_head = addr.wrapping_add(width);
            self.next_seq_addr = self.pf_head;
            self.log_access(addr, width, 'P', cost);
            self.cycles += cost;
            return;
        }
        let c = self.access_cost(addr, width);
        self.log_access(addr, width, 'R', c);
        self.cycles += c;
        if self.io.waitcnt & 0x4000 != 0 {
            self.pf_active = true;
            self.pf_head = addr.wrapping_add(width);
            self.pf_count = 0;
            self.pf_progress = 0;
        } else {
            self.pf_active = false;
        }
    }

    /// Internal (I) cycles: the bus is idle and the prefetcher runs; the
    /// next access is non-sequential. (Lockstep vs the reference: an
    /// instruction's I cycles come before its opcode fetch, which is
    /// then N.)
    #[inline(always)]
    pub fn idle(&mut self, cycles: u32) {
        if cycles == 0 {
            return;
        }
        self.next_seq_addr = u32::MAX;
        self.prefetch_advance(cycles);
        self.log_access(0, 0, 'I', cycles);
        self.cycles += cycles;
    }

    /// The extra cycle of a pipeline flush, charged as a sequential
    /// access at the branch destination.
    #[inline(always)]
    pub fn branch_dummy_cycle(&mut self, target: u32, width: u32) {
        let mode = waitstates::SEQUENTIAL | if width == 4 { waitstates::WORD } else { 0 };
        let c = self.ws.get(target | 2, mode) as u32;
        if !Self::is_gamepak_rom(target) {
            self.prefetch_advance(c);
        }
        self.log_access(target, width, 'B', c);
        self.cycles += c;
    }

    /// DMA unit access: its source and destination are separate
    /// sequential streams (2N + 2(n-1)S per GBATEK), unlike the CPU.
    #[inline(always)]
    fn dma_cost(&mut self, addr: u32, width: u32, seq: bool) {
        let mode = if seq { waitstates::SEQUENTIAL } else { 0 }
            | if width == 4 { waitstates::WORD } else { 0 };
        let c = self.ws.get(addr, mode) as u32;
        if (addr >> 24) >= 0x08 {
            self.pf_active = false;
            self.pf_count = 0;
            self.pf_progress = 0;
        } else {
            self.prefetch_advance(c);
        }
        self.log_access(addr, width, 'M', c);
        self.cycles += c;
    }

    /// A pipeline flush discards the prefetch buffer (lockstep vs
    /// reference).
    #[inline(always)]
    pub fn prefetch_branch_reset(&mut self) {
        let (a, p, c) = (self.pf_active, self.pf_progress, self.pf_count);
        self.log_access(p, c, if a { 'X' } else { 'x' }, 0);
        let _ = (a, p, c);
        self.pf_active = false;
        self.pf_count = 0;
        self.pf_progress = 0;
    }

    /// Make the next access non-sequential (pipeline refill).
    #[inline(always)]
    pub fn break_sequence(&mut self) {
        self.next_seq_addr = u32::MAX;
    }

    #[inline(always)]
    pub fn take_cycles(&mut self) -> u32 {
        core::mem::take(&mut self.cycles)
    }

    #[inline(always)]
    pub fn fetch16(&mut self, addr: u32) -> u16 {
        let addr = addr & !1;
        self.code_access(addr, 2);
        self.fetch_in_bios = addr < 0x4000;
        let v = self.read16_raw(addr);
        // Open bus after a THUMB fetch (GBATEK "Unpredictable Things"):
        // 16-bit buses duplicate the opcode; IWRAM/BIOS/OAM (32-bit)
        // expose the neighbouring halfword of the same word.
        let v32 = v as u32;
        self.open_bus = match addr >> 24 {
            0x00 | 0x03 | 0x07 => {
                if addr & 2 == 0 {
                    v32 | ((self.read16_raw(addr + 2) as u32) << 16)
                } else {
                    (self.read16_raw(addr - 2) as u32) | (v32 << 16)
                }
            }
            _ => v32 | (v32 << 16),
        };
        if self.fetch_in_bios {
            self.bios_latch = self.read32_raw(addr & !3);
        }
        v
    }

    #[inline(always)]
    pub fn fetch32(&mut self, addr: u32) -> u32 {
        let addr = addr & !3;
        self.code_access(addr, 4);
        self.fetch_in_bios = addr < 0x4000;
        let v = self.read32_raw(addr);
        self.open_bus = v;
        if self.fetch_in_bios {
            self.bios_latch = v;
        }
        v
    }

    #[inline(always)]
    pub fn read8(&mut self, addr: u32, _pc: u32) -> u8 {
        self.data_access(addr, 1);
        self.read8_raw(addr)
    }

    #[inline(always)]
    pub fn read16(&mut self, addr: u32, _pc: u32) -> u16 {
        self.data_access(addr & !1, 2);
        self.read16_raw(addr)
    }

    #[inline(always)]
    pub fn read32(&mut self, addr: u32, _pc: u32) -> u32 {
        self.data_access(addr & !3, 4);
        self.read32_raw(addr)
    }

    #[inline(always)]
    pub fn write8(&mut self, addr: u32, val: u8) {
        self.data_access(addr, 1);
        self.write8_raw(addr, val)
    }

    #[inline(always)]
    pub fn write16(&mut self, addr: u32, val: u16) {
        self.data_access(addr & !1, 2);
        self.write16_raw(addr, val)
    }

    #[inline(always)]
    pub fn write32(&mut self, addr: u32, val: u32) {
        self.data_access(addr & !3, 4);
        self.write32_raw(addr, val)
    }

    #[inline(always)]
    fn vram_mirror_offset(&self, addr: u32) -> usize {
        // 96 KiB VRAM in a 128 KiB window: 0x18000..0x1FFFF mirrors the
        // upper 32 KiB (0x10000..0x17FFF).
        // In bitmap modes (3-5) 0x18000..0x1BFFF would mirror BG memory
        // and is instead unmapped: reads 0, writes ignored.
        let off = (addr & 0x1FFFF) as usize;
        if off < 0x18000 {
            off
        } else if self.ppu.dispcnt & 7 >= 3 && off < 0x1C000 {
            VRAM_UNMAPPED
        } else {
            off - 0x8000
        }
    }

    /// BIOS reads are only allowed while executing inside the BIOS;
    /// otherwise the last fetched BIOS opcode is returned.
    #[inline(always)]
    fn bios_word(&self, addr: u32) -> u32 {
        if addr >= 0x4000 {
            return self.open_bus;
        }
        if !self.fetch_in_bios {
            return self.bios_latch;
        }
        let o = (addr & 0x3FFC) as usize;
        u32::from_le_bytes([self.bios[o], self.bios[o + 1], self.bios[o + 2], self.bios[o + 3]])
    }

    /// Reads past the end of the cartridge return the address bus value
    /// (address/2 in each halfword).
    #[inline(always)]
    fn rom_oob_half(addr: u32) -> u16 {
        ((addr & 0x01FF_FFFE) >> 1) as u16
    }

    fn read8_raw(&mut self, addr: u32) -> u8 {
        let page = (addr >> 24) as usize;
        let val = match page {
            0x00 => (self.bios_word(addr) >> ((addr & 3) * 8)) as u8,
            0x02 => self.ewram[(addr as usize) & 0x3FFFF],
            0x03 => self.iwram[(addr as usize) & 0x7FFF],
            0x04 => self.read_io8(addr),
            0x05 => self.palette[(addr as usize) & 0x3FF],
            0x06 => {
                let off = self.vram_mirror_offset(addr);
                if off == VRAM_UNMAPPED { 0 } else { self.vram[off] }
            }
            0x07 => self.oam[(addr as usize) & 0x3FF],
            0x08..=0x0D => {
                let off = (addr & 0x01FF_FFFF) as usize;
                if off < self.rom.len() {
                    self.rom[off]
                } else {
                    (Self::rom_oob_half(addr) >> ((addr & 1) * 8)) as u8
                }
            }
            0x0E..=0x0F => self.sram[(addr as usize) & 0xFFFF],
            _ => (self.open_bus >> ((addr & 3) * 8)) as u8,
        };
        val
    }

    fn read16_raw(&mut self, addr: u32) -> u16 {
        let addr = addr & !1;
        let page = (addr >> 24) as usize;
        let val = match page {
            0x00 => (self.bios_word(addr) >> ((addr & 2) * 8)) as u16,
            0x02 => {
                let off = (addr as usize) & 0x3FFFF;
                u16::from_le_bytes([self.ewram[off], self.ewram[off + 1]])
            }
            0x03 => {
                let off = (addr as usize) & 0x7FFF;
                u16::from_le_bytes([self.iwram[off], self.iwram[off + 1]])
            }
            0x04 => self.read_io16(addr),
            0x05 => {
                let off = (addr as usize) & 0x3FF;
                u16::from_le_bytes([self.palette[off], self.palette[off + 1]])
            }
            0x06 => {
                let off = self.vram_mirror_offset(addr);
                if off == VRAM_UNMAPPED { return 0; }
                u16::from_le_bytes([self.vram[off], self.vram[off + 1]])
            }
            0x07 => {
                let off = (addr as usize) & 0x3FF;
                u16::from_le_bytes([self.oam[off], self.oam[off + 1]])
            }
            0x08..=0x0D => {
                let off = (addr & 0x01FF_FFFF) as usize;
                if off + 1 < self.rom.len() {
                    u16::from_le_bytes([self.rom[off], self.rom[off + 1]])
                } else {
                    Self::rom_oob_half(addr)
                }
            }
            0x0E..=0x0F => {
                let off = (addr as usize) & 0xFFFF;
                let b = self.sram[off];
                (b as u16) | ((b as u16) << 8)
            }
            _ => (self.open_bus >> ((addr & 2) * 8)) as u16,
        };
        val
    }

    fn read32_raw(&mut self, addr: u32) -> u32 {
        let addr = addr & !3;
        let page = (addr >> 24) as usize;
        let val = match page {
            0x00 => self.bios_word(addr),
            0x02 => {
                let off = (addr as usize) & 0x3FFFF;
                u32::from_le_bytes([self.ewram[off], self.ewram[off + 1], self.ewram[off + 2], self.ewram[off + 3]])
            }
            0x03 => {
                let off = (addr as usize) & 0x7FFF;
                u32::from_le_bytes([self.iwram[off], self.iwram[off + 1], self.iwram[off + 2], self.iwram[off + 3]])
            }
            0x04 => {
                let low = self.read_io16(addr) as u32;
                let high = self.read_io16(addr + 2) as u32;
                low | (high << 16)
            }
            0x05 => {
                let off = (addr as usize) & 0x3FF;
                u32::from_le_bytes([self.palette[off], self.palette[off + 1], self.palette[off + 2], self.palette[off + 3]])
            }
            0x06 => {
                let off = self.vram_mirror_offset(addr);
                if off == VRAM_UNMAPPED { return 0; }
                u32::from_le_bytes([self.vram[off], self.vram[off + 1], self.vram[off + 2], self.vram[off + 3]])
            }
            0x07 => {
                let off = (addr as usize) & 0x3FF;
                u32::from_le_bytes([self.oam[off], self.oam[off + 1], self.oam[off + 2], self.oam[off + 3]])
            }
            0x08..=0x0D => {
                let off = (addr & 0x01FF_FFFF) as usize;
                if off + 3 < self.rom.len() {
                    u32::from_le_bytes([self.rom[off], self.rom[off + 1], self.rom[off + 2], self.rom[off + 3]])
                } else {
                    (Self::rom_oob_half(addr) as u32) | ((Self::rom_oob_half(addr + 2) as u32) << 16)
                }
            }
            0x0E..=0x0F => {
                let off = (addr as usize) & 0xFFFF;
                let b = self.sram[off] as u32;
                b | (b << 8) | (b << 16) | (b << 24)
            }
            _ => self.open_bus,
        };
        val
    }

    fn write8_raw(&mut self, addr: u32, val: u8) {
        let page = (addr >> 24) as usize;
        match page {
            0x02 => self.ewram[(addr as usize) & 0x3FFFF] = val,
            0x03 => self.iwram[(addr as usize) & 0x7FFF] = val,
            0x04 => self.write_io8(addr, val),
            0x05 => {
                // Byte writes to palette duplicate to 16-bit
                let off = (addr as usize) & 0x3FE;
                self.palette[off] = val;
                self.palette[off + 1] = val;
            }
            0x06 => {
                let off = self.vram_mirror_offset(addr);
                if off == VRAM_UNMAPPED { return; }
                let obj_base = if self.ppu.dispcnt & 7 >= 3 { 0x14000 } else { 0x10000 };
                let is_obj = off >= obj_base;
                if !is_obj {
                    let aligned = off & !1;
                    self.vram[aligned] = val;
                    self.vram[aligned + 1] = val;
                }
            }
            0x07 => {} // Byte writes to OAM ignored
            0x0E..=0x0F => self.sram[(addr as usize) & 0xFFFF] = val,
            _ => {}
        }
    }

    fn write16_raw(&mut self, addr: u32, val: u16) {
        let addr = addr & !1;
        let page = (addr >> 24) as usize;
        let bytes = val.to_le_bytes();
        match page {
            0x02 => {
                let off = (addr as usize) & 0x3FFFF;
                self.ewram[off] = bytes[0];
                self.ewram[off + 1] = bytes[1];
            }
            0x03 => {
                let off = (addr as usize) & 0x7FFF;
                self.iwram[off] = bytes[0];
                self.iwram[off + 1] = bytes[1];
            }
            0x04 => self.write_io16(addr, val),
            0x05 => {
                let off = (addr as usize) & 0x3FF;
                self.palette[off] = bytes[0];
                self.palette[off + 1] = bytes[1];
            }
            0x06 => {
                let off = self.vram_mirror_offset(addr);
                if off == VRAM_UNMAPPED { return; }
                self.vram[off] = bytes[0];
                self.vram[off + 1] = bytes[1];
            }
            0x07 => {
                let off = (addr as usize) & 0x3FF;
                self.oam[off] = bytes[0];
                self.oam[off + 1] = bytes[1];
            }
            0x0E..=0x0F => {
                self.sram[(addr as usize) & 0xFFFF] = bytes[0];
            }
            _ => {}
        }
    }

    fn write32_raw(&mut self, addr: u32, val: u32) {
        let addr = addr & !3;
        let page = (addr >> 24) as usize;
        let bytes = val.to_le_bytes();
        match page {
            0x02 => {
                let off = (addr as usize) & 0x3FFFF;
                self.ewram[off..off + 4].copy_from_slice(&bytes);
            }
            0x03 => {
                let off = (addr as usize) & 0x7FFF;
                self.iwram[off..off + 4].copy_from_slice(&bytes);
            }
            0x04 => {
                self.write_io16(addr, val as u16);
                self.write_io16(addr + 2, (val >> 16) as u16);
            }
            0x05 => {
                let off = (addr as usize) & 0x3FF;
                self.palette[off..off + 4].copy_from_slice(&bytes);
            }
            0x06 => {
                let off = self.vram_mirror_offset(addr);
                if off == VRAM_UNMAPPED { return; }
                self.vram[off..off + 4].copy_from_slice(&bytes);
            }
            0x07 => {
                let off = (addr as usize) & 0x3FF;
                self.oam[off..off + 4].copy_from_slice(&bytes);
            }
            0x0E..=0x0F => {
                self.sram[(addr as usize) & 0xFFFF] = bytes[0];
            }
            _ => {}
        }
    }

    fn read_io8(&mut self, addr: u32) -> u8 {
        let half = self.read_io16(addr & !1);
        if addr & 1 == 0 {
            (half & 0xFF) as u8
        } else {
            ((half >> 8) & 0xFF) as u8
        }
    }

    fn read_io16(&mut self, addr: u32) -> u16 {
        let open_bus_half = (self.open_bus >> ((addr & 2) * 8)) as u16;
        match addr {
            0x04000000 => self.ppu.dispcnt,
            0x04000004 => self.ppu.dispstat & 0xFF3F, // bits 6, 7 are 0
            0x04000006 => self.ppu.vcount & 0x00FF,   // upper byte is 0
            0x04000008 => self.ppu.bgcnt[0] & 0xDFFF,
            0x0400000A => self.ppu.bgcnt[1] & 0xDFFF,
            0x0400000C => self.ppu.bgcnt[2],
            0x0400000E => self.ppu.bgcnt[3],
            0x04000048 => self.ppu.winin & 0x3F3F,
            0x0400004A => self.ppu.winout & 0x3F3F,
            0x04000050 => self.ppu.bldcnt & 0x3FFF,
            0x04000060..=0x040000AF => {
                let low = self.apu.read_io(addr) as u16;
                let high = self.apu.read_io(addr + 1) as u16;
                low | (high << 8)
            }
            0x040000B0..=0x040000DF => self.dma.read16(addr, open_bus_half),
            0x04000100..=0x0400010F => {
                self.sync_timers();
                self.timers.read16(addr)
            }
            0x04000130 => 0xFC00 | (self.io.keyinput & 0x03FF),
            0x04000132 => self.io.keycnt,
            0x04000200 => self.io.ie,
            0x04000202 => self.io.if_,
            0x04000204 => self.io.waitcnt,
            0x04000208 => self.io.ime as u16,
            0x04000300 => (self.io.postflg as u16) | (open_bus_half & 0xFF00),
            // Readable LCD registers not handled above, then write-only
            // registers (read as 0); truly unused addresses are open bus.
            0x04000002 => self.io_shadow[1],
            0x04000052 => self.io_shadow[0x52 >> 1] & 0x1F1F,
            0x04000134 => self.io_shadow[0x134 >> 1],
            0x04000120..=0x0400015A => self.io_shadow[((addr & 0x3FE) >> 1) as usize],
            0x04000206 | 0x0400020A | 0x04000302 => 0,
            _ => open_bus_half,
        }
    }

    fn write_io8(&mut self, addr: u32, val: u8) {
        if (0x0400_0200..=0x0400_020B).contains(&addr) {
            self.write_io8_inner(addr, val);
            let t = self.access_time();
            self.update_irq_line(t);
            return;
        }
        self.write_io8_inner(addr, val)
    }

    fn write_io8_inner(&mut self, addr: u32, val: u8) {
        match addr {
            0x04000000 => self.ppu.dispcnt = (self.ppu.dispcnt & 0xFF00) | (val as u16),
            0x04000001 => self.ppu.dispcnt = (self.ppu.dispcnt & 0x00FF) | ((val as u16) << 8),
            0x04000004 => self.ppu.dispstat = (self.ppu.dispstat & 0xFF07) | ((val as u16) & 0xF8),
            0x04000005 => self.ppu.dispstat = (self.ppu.dispstat & 0x00FF) | ((val as u16) << 8),
            0x04000060..=0x040000AF => self.apu.write_io(addr, val),
            0x040000B0..=0x040000DF => {
                self.dma.write8(addr, val);
                self.check_dma_immediate();
            }
            0x04000100..=0x0400010F => {
                self.sync_timers();
                self.timers.write8(addr, val)
            }
            0x04000200 => self.io.ie = (self.io.ie & 0xFF00) | (val as u16),
            0x04000201 => self.io.ie = (self.io.ie & 0x00FF) | ((val as u16) << 8),
            0x04000202 => self.io.if_ &= !(val as u16),
            0x04000203 => self.io.if_ &= !((val as u16) << 8),
            0x04000208 => self.io.ime = val & 1 != 0,
            0x04000300 => self.io.postflg = val,
            0x04000301 => {
                self.io.haltcnt = val & 0x80 == 0;
                if val & 0x80 == 0 {
                    self.halt_request = true;
                }
            }
            0x04000000..=0x040003FF => {
                // Merge into the last-written halfword so byte writes to
                // 16-bit (often write-only) registers behave.
                let idx = ((addr & 0x3FE) >> 1) as usize;
                let cur = self.io_shadow[idx];
                let merged = if addr & 1 == 0 {
                    (cur & 0xFF00) | val as u16
                } else {
                    (cur & 0x00FF) | ((val as u16) << 8)
                };
                self.write_io16(addr & !1, merged);
            }
            _ => {}
        }
    }

    fn write_io16(&mut self, addr: u32, val: u16) {
        self.write_io16_inner(addr, val);
        if (0x0400_0200..=0x0400_020B).contains(&addr) {
            let t = self.access_time();
            self.update_irq_line(t);
        }
    }

    fn write_io16_inner(&mut self, addr: u32, val: u16) {
        if (addr & 0x00FF_FFFF) < 0x400 {
            self.io_shadow[((addr & 0x3FE) >> 1) as usize] = val;
        }
        match addr {
            0x04000000 => self.ppu.dispcnt = val,
            0x04000004 => self.ppu.dispstat = (self.ppu.dispstat & 7) | (val & !7),
            0x04000008 => self.ppu.bgcnt[0] = val,
            0x0400000A => self.ppu.bgcnt[1] = val,
            0x0400000C => self.ppu.bgcnt[2] = val,
            0x0400000E => self.ppu.bgcnt[3] = val,
            0x04000010 => self.ppu.bghofs[0] = val & 0x1FF,
            0x04000012 => self.ppu.bgvofs[0] = val & 0x1FF,
            0x04000014 => self.ppu.bghofs[1] = val & 0x1FF,
            0x04000016 => self.ppu.bgvofs[1] = val & 0x1FF,
            0x04000018 => self.ppu.bghofs[2] = val & 0x1FF,
            0x0400001A => self.ppu.bgvofs[2] = val & 0x1FF,
            0x0400001C => self.ppu.bghofs[3] = val & 0x1FF,
            0x0400001E => self.ppu.bgvofs[3] = val & 0x1FF,

            0x04000020 => self.ppu.bg2pa = val as i16,
            0x04000022 => self.ppu.bg2pb = val as i16,
            0x04000024 => self.ppu.bg2pc = val as i16,
            0x04000026 => self.ppu.bg2pd = val as i16,
            0x04000028 => {
                let low = val as i32;
                self.ppu.bg2x = (self.ppu.bg2x & !0xFFFF) | (low & 0xFFFF);
                self.ppu.bg2_cur_x = self.ppu.bg2x;
            }
            0x0400002A => {
                let high = ((val as i16 as i32) << 16) as i32;
                self.ppu.bg2x = (self.ppu.bg2x & 0xFFFF) | high;
                self.ppu.bg2_cur_x = self.ppu.bg2x;
            }
            0x0400002C => {
                let low = val as i32;
                self.ppu.bg2y = (self.ppu.bg2y & !0xFFFF) | (low & 0xFFFF);
                self.ppu.bg2_cur_y = self.ppu.bg2y;
            }
            0x0400002E => {
                let high = ((val as i16 as i32) << 16) as i32;
                self.ppu.bg2y = (self.ppu.bg2y & 0xFFFF) | high;
                self.ppu.bg2_cur_y = self.ppu.bg2y;
            }

            0x04000030 => self.ppu.bg3pa = val as i16,
            0x04000032 => self.ppu.bg3pb = val as i16,
            0x04000034 => self.ppu.bg3pc = val as i16,
            0x04000036 => self.ppu.bg3pd = val as i16,
            0x04000038 => {
                let low = val as i32;
                self.ppu.bg3x = (self.ppu.bg3x & !0xFFFF) | (low & 0xFFFF);
                self.ppu.bg3_cur_x = self.ppu.bg3x;
            }
            0x0400003A => {
                let high = ((val as i16 as i32) << 16) as i32;
                self.ppu.bg3x = (self.ppu.bg3x & 0xFFFF) | high;
                self.ppu.bg3_cur_x = self.ppu.bg3x;
            }
            0x0400003C => {
                let low = val as i32;
                self.ppu.bg3y = (self.ppu.bg3y & !0xFFFF) | (low & 0xFFFF);
                self.ppu.bg3_cur_y = self.ppu.bg3y;
            }
            0x0400003E => {
                let high = ((val as i16 as i32) << 16) as i32;
                self.ppu.bg3y = (self.ppu.bg3y & 0xFFFF) | high;
                self.ppu.bg3_cur_y = self.ppu.bg3y;
            }

            0x04000040 => self.ppu.win0h = val,
            0x04000042 => self.ppu.win1h = val,
            0x04000044 => self.ppu.win0v = val,
            0x04000046 => self.ppu.win1v = val,
            0x04000048 => self.ppu.winin = val,
            0x0400004A => self.ppu.winout = val,
            0x0400004C => self.ppu.mosaic = val,
            0x04000050 => self.ppu.bldcnt = val,
            0x04000052 => self.ppu.bldalpha = val,
            0x04000054 => self.ppu.bldy = val,

            0x04000060..=0x040000AF => {
                self.apu.write_io(addr, (val & 0xFF) as u8);
                self.apu.write_io(addr + 1, (val >> 8) as u8);
            }

            0x040000B0..=0x040000DF => {
                self.dma.write16(addr, val);
                self.check_dma_immediate();
            }

            0x04000100..=0x0400010F => {
                self.sync_timers();
                self.timers.write16(addr, val)
            }
            0x04000132 => self.io.keycnt = val,
            0x04000200 => self.io.ie = val,
            0x04000202 => self.io.if_ &= !val,
            0x04000204 => {
                self.io.waitcnt = val;
                self.ws.write_waitcnt(val);
            }
            0x04000208 => self.io.ime = val & 1 != 0,
            _ => {}
        }
    }

    pub fn check_dma_immediate(&mut self) {
        for ch in 0..4 {
            if self.dma.channels[ch].enabled && self.dma.channels[ch].trigger == DmaTrigger::Immediate {
                let can_run = !(0..=ch).any(|i| self.dma.channels[i].is_running);
                if can_run {
                    self.execute_dma_channel(ch);
                }
            }
        }
    }

    pub fn trigger_dma(&mut self, trigger: DmaTrigger) {
        for ch in 0..4 {
            if self.dma.channels[ch].enabled && self.dma.channels[ch].trigger == trigger {
                let can_run = !(0..=ch).any(|i| self.dma.channels[i].is_running);
                if can_run {
                    self.execute_dma_channel(ch);
                }
            }
        }
    }

    pub fn trigger_dma_fifo(&mut self, fifo_a: bool, fifo_b: bool) {
        for ch in 1..=2 {
            if self.dma.channels[ch].enabled && self.dma.channels[ch].trigger == DmaTrigger::Special {
                let can_run = !(0..=ch).any(|i| self.dma.channels[i].is_running);
                if can_run {
                    let dad = self.dma.channels[ch].dad;
                    if fifo_a && dad == 0x040000A0 {
                        self.execute_dma_channel_fifo(ch, 0x040000A0);
                    } else if fifo_b && dad == 0x040000A4 {
                        self.execute_dma_channel_fifo(ch, 0x040000A4);
                    }
                }
            }
        }
    }

    fn execute_dma_channel(&mut self, ch: usize) {
        self.dma_ran = true;
        self.dma.channels[ch].is_running = true;
        // 2-cycle start delay + 2 internal cycles (GBATEK; total matches
        // the reference under lockstep).
        self.idle(DMA_OVERHEAD_CYCLES);
        let is_32 = self.dma.channels[ch].is_32bit;
        let count = self.dma.channels[ch].cur_count;
        let step = if is_32 { 4 } else { 2 };

        let mut src = self.dma.channels[ch].cur_sad;
        let mut dest = self.dma.channels[ch].cur_dad;
        let src_adj = self.dma.channels[ch].src_adj;
        let dest_adj = self.dma.channels[ch].dest_adj;

        // Game Pak sources always increment.
        let src_adj = if Self::is_gamepak_rom(src) { 0 } else { src_adj };

        for k in 0..count {
            let seq = k > 0;
            if is_32 {
                self.dma_cost(src, 4, seq);
                if src >= 0x0200_0000 {
                    self.dma_latch = self.read32_raw(src);
                }
                let v = self.dma_latch;
                self.dma_cost(dest, 4, seq);
                self.write32_raw(dest, v);
            } else {
                self.dma_cost(src, 2, seq);
                if src >= 0x0200_0000 {
                    let h = self.read16_raw(src) as u32;
                    self.dma_latch = h | (h << 16);
                }
                let v = (self.dma_latch >> ((dest & 2) * 8)) as u16;
                self.dma_cost(dest, 2, seq);
                self.write16_raw(dest, v);
            }

            match src_adj {
                0 => src = src.wrapping_add(step),
                1 => src = src.wrapping_sub(step),
                _ => {}
            }

            match dest_adj {
                0 | 3 => dest = dest.wrapping_add(step),
                1 => dest = dest.wrapping_sub(step),
                _ => {}
            }
        }

        self.dma.channels[ch].cur_sad = src;
        if dest_adj == 3 {
            self.dma.channels[ch].cur_dad = self.dma.channels[ch].dad;
        } else {
            self.dma.channels[ch].cur_dad = dest;
        }

        if self.dma.channels[ch].irq_enable {
            self.io.if_ |= 1 << (8 + ch);
            let t = self.access_time();
            self.update_irq_line(t);
        }

        if !self.dma.channels[ch].repeat || self.dma.channels[ch].trigger == DmaTrigger::Immediate {
            self.dma.channels[ch].enabled = false;
        } else {
            self.dma.channels[ch].reload_count(ch);
        }

        self.dma.channels[ch].is_running = false;
        // Check if any deferred lower-priority immediate DMAs can run now
        self.check_dma_immediate();
    }

    fn execute_dma_channel_fifo(&mut self, ch: usize, dest_fifo: u32) {
        self.dma_ran = true;
        self.idle(2);
        let mut src = self.dma.channels[ch].cur_sad;
        let src_adj = self.dma.channels[ch].src_adj;

        // FIFO DMA always transfers 4 words (16 bytes)
        for k in 0..4 {
            self.dma_cost(src, 4, k > 0);
            let val = self.read32_raw(src);
            self.dma_cost(dest_fifo, 4, k > 0);
            self.write32_raw(dest_fifo, val);

            match src_adj {
                0 => src = src.wrapping_add(4),
                1 => src = src.wrapping_sub(4),
                _ => {}
            }
        }

        self.dma.channels[ch].cur_sad = src;
        if self.dma.channels[ch].irq_enable {
            self.io.if_ |= 1 << (8 + ch);
            let t = self.access_time();
            self.update_irq_line(t);
        }

        if !self.dma.channels[ch].repeat {
            self.dma.channels[ch].enabled = false;
        }
    }
}
