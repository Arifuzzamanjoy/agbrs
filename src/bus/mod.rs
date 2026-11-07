//! GBA Memory Bus, MMIO Dispatch, and Peripheral Integration.

pub mod io;
pub mod waitstates;

use io::IoRegisters;
use waitstates::WaitStates;
use crate::ppu::Ppu;
use crate::apu::Apu;
use crate::dma::{DmaController, DmaTrigger};
use crate::timer::Timers;

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
    }

    #[inline(always)]
    fn vram_mirror_offset(&self, addr: u32) -> usize {
        let off = (addr & 0x1FFFF) as usize;
        if off < 0x18000 {
            off
        } else {
            let bg_mode = self.ppu.dispcnt & 7;
            if bg_mode >= 3 {
                0x14000 + (off & 0x3FFF)
            } else {
                0x10000 + (off & 0x7FFF)
            }
        }
    }

    pub fn read8(&mut self, addr: u32, _pc: u32) -> u8 {
        let page = (addr >> 24) as usize;
        let val = match page {
            0x00 => {
                if addr < 0x4000 {
                    self.bios[addr as usize]
                } else {
                    self.open_bus as u8
                }
            }
            0x02 => self.ewram[(addr as usize) & 0x3FFFF],
            0x03 => self.iwram[(addr as usize) & 0x7FFF],
            0x04 => self.read_io8(addr),
            0x05 => self.palette[(addr as usize) & 0x3FF],
            0x06 => self.vram[self.vram_mirror_offset(addr)],
            0x07 => self.oam[(addr as usize) & 0x3FF],
            0x08..=0x0D => {
                let off = (addr & 0x01FF_FFFF) as usize;
                if off < self.rom.len() {
                    self.rom[off]
                } else {
                    self.open_bus as u8
                }
            }
            0x0E..=0x0F => self.sram[(addr as usize) & 0xFFFF],
            _ => (self.open_bus >> ((addr & 3) * 8)) as u8,
        };
        self.open_bus = (self.open_bus & !0xFF) | (val as u32);
        val
    }

    pub fn read16(&mut self, addr: u32, _pc: u32) -> u16 {
        let addr = addr & !1;
        let page = (addr >> 24) as usize;
        let val = match page {
            0x00 => {
                if addr < 0x4000 {
                    u16::from_le_bytes([self.bios[addr as usize], self.bios[addr as usize + 1]])
                } else {
                    self.open_bus as u16
                }
            }
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
                    self.open_bus as u16
                }
            }
            0x0E..=0x0F => {
                let off = (addr as usize) & 0xFFFF;
                let b = self.sram[off];
                (b as u16) | ((b as u16) << 8)
            }
            _ => (self.open_bus >> ((addr & 2) * 8)) as u16,
        };
        self.open_bus = (self.open_bus & !0xFFFF) | (val as u32);
        val
    }

    pub fn read32(&mut self, addr: u32, _pc: u32) -> u32 {
        let addr = addr & !3;
        let page = (addr >> 24) as usize;
        let val = match page {
            0x00 => {
                if addr < 0x4000 {
                    let off = addr as usize;
                    u32::from_le_bytes([self.bios[off], self.bios[off + 1], self.bios[off + 2], self.bios[off + 3]])
                } else {
                    self.open_bus
                }
            }
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
                    self.open_bus
                }
            }
            0x0E..=0x0F => {
                let off = (addr as usize) & 0xFFFF;
                let b = self.sram[off] as u32;
                b | (b << 8) | (b << 16) | (b << 24)
            }
            _ => self.open_bus,
        };
        self.open_bus = val;
        val
    }

    pub fn write8(&mut self, addr: u32, val: u8) {
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
                let is_obj = off >= 0x10000;
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

    pub fn write16(&mut self, addr: u32, val: u16) {
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

    pub fn write32(&mut self, addr: u32, val: u32) {
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
            0x04000008 => self.ppu.bgcnt[0],
            0x0400000A => self.ppu.bgcnt[1],
            0x0400000C => self.ppu.bgcnt[2],
            0x0400000E => self.ppu.bgcnt[3],
            0x04000048 => self.ppu.winin,
            0x0400004A => self.ppu.winout,
            0x04000050 => self.ppu.bldcnt,
            0x04000060..=0x040000AF => {
                let low = self.apu.read_io(addr) as u16;
                let high = self.apu.read_io(addr + 1) as u16;
                low | (high << 8)
            }
            0x040000B0..=0x040000DF => self.dma.read16(addr, open_bus_half),
            0x04000100..=0x0400010F => self.timers.read16(addr),
            0x04000130 => 0xFC00 | (self.io.keyinput & 0x03FF),
            0x04000132 => self.io.keycnt,
            0x04000200 => self.io.ie,
            0x04000202 => self.io.if_,
            0x04000204 => self.io.waitcnt,
            0x04000208 => self.io.ime as u16,
            0x04000300 => (self.io.postflg as u16) | (open_bus_half & 0xFF00),
            _ => open_bus_half,
        }
    }

    fn write_io8(&mut self, addr: u32, val: u8) {
        match addr {
            0x04000000 => self.ppu.dispcnt = (self.ppu.dispcnt & 0xFF00) | (val as u16),
            0x04000001 => self.ppu.dispcnt = (self.ppu.dispcnt & 0x00FF) | ((val as u16) << 8),
            0x04000004 => self.ppu.dispstat = (self.ppu.dispstat & 0xFF00) | ((val as u16) & 0xF8),
            0x04000005 => self.ppu.dispstat = (self.ppu.dispstat & 0x00FF) | ((val as u16) << 8),
            0x04000060..=0x040000AF => self.apu.write_io(addr, val),
            0x040000B0..=0x040000DF => {
                self.dma.write8(addr, val);
                self.check_dma_immediate();
            }
            0x04000100..=0x0400010F => self.timers.write8(addr, val),
            0x04000200 => self.io.ie = (self.io.ie & 0xFF00) | (val as u16),
            0x04000201 => self.io.ie = (self.io.ie & 0x00FF) | ((val as u16) << 8),
            0x04000202 => self.io.if_ &= !(val as u16),
            0x04000203 => self.io.if_ &= !((val as u16) << 8),
            0x04000208 => self.io.ime = val & 1 != 0,
            0x04000300 => self.io.postflg = val,
            0x04000301 => self.io.haltcnt = val & 0x80 == 0,
            _ => {}
        }
    }

    fn write_io16(&mut self, addr: u32, val: u16) {
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

            0x04000100..=0x0400010F => self.timers.write16(addr, val),
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
        self.dma.channels[ch].is_running = true;
        let is_32 = self.dma.channels[ch].is_32bit;
        let count = self.dma.channels[ch].cur_count;
        let step = if is_32 { 4 } else { 2 };

        let mut src = self.dma.channels[ch].cur_sad;
        let mut dest = self.dma.channels[ch].cur_dad;
        let src_adj = self.dma.channels[ch].src_adj;
        let dest_adj = self.dma.channels[ch].dest_adj;

        for _ in 0..count {
            if is_32 {
                let val = self.read32(src, src);
                self.write32(dest, val);
            } else {
                let val = self.read16(src, src);
                self.write16(dest, val);
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
        }

        if !self.dma.channels[ch].repeat {
            self.dma.channels[ch].enabled = false;
        }

        self.dma.channels[ch].is_running = false;
        // Check if any deferred lower-priority immediate DMAs can run now
        self.check_dma_immediate();
    }

    fn execute_dma_channel_fifo(&mut self, ch: usize, dest_fifo: u32) {
        let mut src = self.dma.channels[ch].cur_sad;
        let src_adj = self.dma.channels[ch].src_adj;

        // FIFO DMA always transfers 4 words (16 bytes)
        for _ in 0..4 {
            let val = self.read32(src, src);
            self.write32(dest_fifo, val);

            match src_adj {
                0 => src = src.wrapping_add(4),
                1 => src = src.wrapping_sub(4),
                _ => {}
            }
        }

        self.dma.channels[ch].cur_sad = src;
        if self.dma.channels[ch].irq_enable {
            self.io.if_ |= 1 << (8 + ch);
        }

        if !self.dma.channels[ch].repeat {
            self.dma.channels[ch].enabled = false;
        }
    }
}
