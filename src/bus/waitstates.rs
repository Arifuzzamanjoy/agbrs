//! GBA memory waitstate LUT, mirroring Mesen2's GbaWaitStates.

pub const SEQUENTIAL: u8 = 1 << 0;
pub const WORD: u8 = 1 << 1;

pub struct WaitStates {
    lut: Box<[u8; 0x400]>,
    prg0: [u8; 2],
    prg1: [u8; 2],
    prg2: [u8; 2],
    sram: u8,
}

impl Default for WaitStates {
    fn default() -> Self {
        Self::new()
    }
}

impl WaitStates {
    pub fn new() -> Self {
        let mut ws = Self {
            lut: Box::new([0u8; 0x400]),
            prg0: [5, 3],
            prg1: [5, 5],
            prg2: [5, 9],
            sram: 5,
        };
        ws.regenerate_lut();
        ws
    }

    pub fn write_waitcnt(&mut self, waitcnt: u16) {
        const N_TABLE: [u8; 4] = [5, 4, 3, 9];
        self.prg0[0] = N_TABLE[((waitcnt >> 2) & 3) as usize];
        self.prg0[1] = if waitcnt & (1 << 4) != 0 { 2 } else { 3 };
        self.prg1[0] = N_TABLE[((waitcnt >> 5) & 3) as usize];
        self.prg1[1] = if waitcnt & (1 << 7) != 0 { 2 } else { 5 };
        self.prg2[0] = N_TABLE[((waitcnt >> 8) & 3) as usize];
        self.prg2[1] = if waitcnt & (1 << 10) != 0 { 2 } else { 9 };
        self.sram = N_TABLE[(waitcnt & 3) as usize];
        self.regenerate_lut();
    }

    fn regenerate_lut(&mut self) {
        for mode in 0u8..4 {
            for i in 0u8..=0xFF {
                let is_seq = (mode & SEQUENTIAL) as usize;
                let is_word = mode & WORD != 0;
                let waitstates: u8 = match i {
                    0x02 => if is_word { 6 } else { 3 }, // EWRAM
                    0x05 | 0x06 => if is_word { 2 } else { 1 }, // VRAM / Palette
                    0x08 | 0x09 => {
                        let first = self.prg0[is_seq];
                        first + if is_word { self.prg0[1] } else { 0 }
                    }
                    0x0A | 0x0B => {
                        let first = self.prg1[is_seq];
                        first + if is_word { self.prg1[1] } else { 0 }
                    }
                    0x0C | 0x0D => {
                        let first = self.prg2[is_seq];
                        first + if is_word { self.prg2[1] } else { 0 }
                    }
                    0x0E | 0x0F => self.sram,
                    _ => 1, // BIOS, IWRAM, I/O, OAM — single cycle
                };
                self.lut[(i as usize) << 2 | mode as usize] = waitstates;
            }
        }
    }

    #[inline(always)]
    pub fn get(&self, addr: u32, mode: u8) -> u8 {
        let seq_mask = if addr & 0x1_FFFF != 0 { SEQUENTIAL } else { 0 };
        let idx = ((addr >> 22) & 0x3FC) | ((mode & (WORD | seq_mask)) as u32);
        self.lut[idx as usize]
    }
}

