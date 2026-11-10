//! Memory access timing, written from GBATEK ("GBA Memory Map" access
//! widths/cycles table and "GBA System Control", WAITCNT 0x4000204).
//!
//! Every access costs `1 + waitstates` cycles. Fixed regions:
//!
//! | Region            | 8/16-bit | 32-bit |
//! |-------------------|----------|--------|
//! | BIOS, IWRAM, I/O, OAM | 1    | 1      |
//! | EWRAM (0x02)      | 3        | 6      |
//! | Palette, VRAM     | 1        | 2      |
//!
//! Game Pak ROM has three mirrors (wait states 0/1/2), each with its own
//! first-access (N) and sequential (S) wait states set by WAITCNT. The
//! bus is 16 bits wide, so a 32-bit access is one access followed by one
//! sequential access. SRAM is 8 bits wide with a single setting.
//! At the start of each 128 KiB ROM block an access is always N.

/// Wait states for WAITCNT's 2-bit N fields (SRAM and WS0/1/2 first access).
const N_WAIT: [u32; 4] = [4, 3, 2, 8];

/// Cycle costs of one Game Pak ROM wait-state area.
#[derive(Clone, Copy)]
struct RomArea {
    first: u32,
    seq: u32,
}

pub struct WaitStates {
    rom: [RomArea; 3],
    sram: u32,
}

impl Default for WaitStates {
    fn default() -> Self {
        Self::new()
    }
}

impl WaitStates {
    /// Power-on: WAITCNT = 0.
    pub fn new() -> Self {
        let mut ws = Self {
            rom: [RomArea { first: 0, seq: 0 }; 3],
            sram: 0,
        };
        ws.write_waitcnt(0);
        ws
    }

    pub fn write_waitcnt(&mut self, waitcnt: u16) {
        let field = |shift: u32| ((waitcnt >> shift) & 3) as usize;
        let bit = |n: u32| waitcnt & (1 << n) != 0;
        self.sram = 1 + N_WAIT[field(0)];
        // Sequential wait states: WS0 2/1, WS1 4/1, WS2 8/1 (bit clear/set).
        let seq_wait = [(bit(4), 2), (bit(7), 4), (bit(10), 8)];
        for (area, n_shift) in [2u32, 5, 8].into_iter().enumerate() {
            let (fast, slow) = seq_wait[area];
            self.rom[area] = RomArea {
                first: 1 + N_WAIT[field(n_shift)],
                seq: 1 + if fast { 1 } else { slow },
            };
        }
    }

    /// Cycles for one CPU or DMA access of `width` bytes (1, 2 or 4).
    /// `sequential` is whether it continues the previous access.
    #[inline(always)]
    pub fn access_cycles(&self, addr: u32, width: u32, sequential: bool) -> u32 {
        let wide = width == 4;
        match addr >> 24 {
            0x02 => if wide { 6 } else { 3 },
            0x05 | 0x06 => if wide { 2 } else { 1 },
            0x08..=0x0D => {
                let area = self.rom[((addr >> 24) as usize - 8) / 2];
                let block_start = addr & 0x1_FFFF == 0;
                let first = if sequential && !block_start { area.seq } else { area.first };
                if wide { first + area.seq } else { first }
            }
            0x0E | 0x0F => self.sram,
            _ => 1,
        }
    }

    /// Cost of one sequential ROM halfword fetch in the area containing
    /// `addr` (used by the prefetch buffer model).
    #[inline(always)]
    pub fn rom_seq_halfword(&self, addr: u32) -> u32 {
        match addr >> 24 {
            0x08..=0x0D => self.rom[((addr >> 24) as usize - 8) / 2].seq,
            _ => self.access_cycles(addr, 2, true),
        }
    }
}
