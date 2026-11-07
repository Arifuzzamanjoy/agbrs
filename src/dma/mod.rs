//! Direct Memory Access (DMA 0..3) Controller.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DmaTrigger {
    Immediate = 0,
    VBlank = 1,
    HBlank = 2,
    Special = 3,
}

pub struct DmaChannel {
    pub sad: u32,
    pub dad: u32,
    pub count: u32,
    pub cur_sad: u32,
    pub cur_dad: u32,
    pub cur_count: u32,

    pub dest_adj: u8,   // 0=Inc, 1=Dec, 2=Fixed, 3=Reload
    pub src_adj: u8,    // 0=Inc, 1=Dec, 2=Fixed
    pub repeat: bool,
    pub is_32bit: bool,
    pub trigger: DmaTrigger,
    pub irq_enable: bool,
    pub enabled: bool,
    pub is_running: bool,
    pub gamepak_drq: bool,
}

impl DmaChannel {
    pub fn new() -> Self {
        Self {
            sad: 0,
            dad: 0,
            count: 0,
            cur_sad: 0,
            cur_dad: 0,
            cur_count: 0,
            dest_adj: 0,
            src_adj: 0,
            repeat: false,
            is_32bit: false,
            trigger: DmaTrigger::Immediate,
            irq_enable: false,
            enabled: false,
            is_running: false,
            gamepak_drq: false,
        }
    }

    pub fn write_control(&mut self, val: u16, ch: usize) {
        let old_enabled = self.enabled;
        self.dest_adj = ((val >> 5) & 3) as u8;
        self.src_adj = ((val >> 7) & 3) as u8;
        self.repeat = (val & (1 << 9)) != 0;
        self.is_32bit = (val & (1 << 10)) != 0;
        self.trigger = match (val >> 12) & 3 {
            0 => DmaTrigger::Immediate,
            1 => DmaTrigger::VBlank,
            2 => DmaTrigger::HBlank,
            _ => DmaTrigger::Special,
        };
        self.irq_enable = (val & (1 << 14)) != 0;
        self.enabled = (val & (1 << 15)) != 0;

        if ch != 3 {
            // Game Pak DRQ (bit 11) exists on DMA3 only.
            self.gamepak_drq = false;
        } else {
            self.gamepak_drq = val & (1 << 11) != 0;
        }
        if !old_enabled && self.enabled {
            // Latch addresses (masked per channel) and count.
            let sad_mask = if ch == 0 { 0x07FF_FFFF } else { 0x0FFF_FFFF };
            let dad_mask = if ch == 3 { 0x0FFF_FFFF } else { 0x07FF_FFFF };
            let align = if self.is_32bit { !3 } else { !1 };
            self.cur_sad = self.sad & sad_mask & align;
            self.cur_dad = self.dad & dad_mask & align;
            self.reload_count(ch);
        }
    }

    pub fn reload_count(&mut self, ch: usize) {
        let (mask, max) = if ch == 3 { (0xFFFF, 0x10000) } else { (0x3FFF, 0x4000) };
        let c = self.count & mask;
        self.cur_count = if c == 0 { max } else { c };
    }

    pub fn read_control(&self) -> u16 {
        ((self.dest_adj as u16) << 5)
            | ((self.src_adj as u16) << 7)
            | ((self.repeat as u16) << 9)
            | ((self.is_32bit as u16) << 10)
            | ((self.gamepak_drq as u16) << 11)
            | (((self.trigger as u16) & 3) << 12)
            | ((self.irq_enable as u16) << 14)
            | ((self.enabled as u16) << 15)
    }
}

pub struct DmaController {
    pub channels: [DmaChannel; 4],
}

impl Default for DmaController {
    fn default() -> Self {
        Self::new()
    }
}

impl DmaController {
    pub fn new() -> Self {
        Self {
            channels: [
                DmaChannel::new(),
                DmaChannel::new(),
                DmaChannel::new(),
                DmaChannel::new(),
            ],
        }
    }

    pub fn write8(&mut self, addr: u32, val: u8) {
        let ch = ((addr - 0x040000B0) / 12) as usize;
        let reg = (addr - 0x040000B0) % 12;
        if ch >= 4 { return; }

        match reg {
            0 => self.channels[ch].sad = (self.channels[ch].sad & 0xFFFF_FF00) | (val as u32),
            1 => self.channels[ch].sad = (self.channels[ch].sad & 0xFFFF_00FF) | ((val as u32) << 8),
            2 => self.channels[ch].sad = (self.channels[ch].sad & 0xFF00_FFFF) | ((val as u32) << 16),
            3 => self.channels[ch].sad = (self.channels[ch].sad & 0x00FF_FFFF) | ((val as u32) << 24),

            4 => self.channels[ch].dad = (self.channels[ch].dad & 0xFFFF_FF00) | (val as u32),
            5 => self.channels[ch].dad = (self.channels[ch].dad & 0xFFFF_00FF) | ((val as u32) << 8),
            6 => self.channels[ch].dad = (self.channels[ch].dad & 0xFF00_FFFF) | ((val as u32) << 16),
            7 => self.channels[ch].dad = (self.channels[ch].dad & 0x00FF_FFFF) | ((val as u32) << 24),

            8 => self.channels[ch].count = (self.channels[ch].count & 0xFF00) | (val as u32),
            9 => self.channels[ch].count = (self.channels[ch].count & 0x00FF) | ((val as u32) << 8),

            10 => {
                let cur = self.channels[ch].read_control();
                self.channels[ch].write_control((cur & 0xFF00) | (val as u16), ch);
            }
            11 => {
                let cur = self.channels[ch].read_control();
                self.channels[ch].write_control((cur & 0x00FF) | ((val as u16) << 8), ch);
            }
            _ => {}
        }
    }

    pub fn write16(&mut self, addr: u32, val: u16) {
        let ch = ((addr - 0x040000B0) / 12) as usize;
        let reg = (addr - 0x040000B0) % 12;
        if ch >= 4 { return; }

        match reg {
            0 => self.channels[ch].sad = (self.channels[ch].sad & 0xFFFF_0000) | (val as u32),
            2 => self.channels[ch].sad = (self.channels[ch].sad & 0x0000_FFFF) | ((val as u32) << 16),
            4 => self.channels[ch].dad = (self.channels[ch].dad & 0xFFFF_0000) | (val as u32),
            6 => self.channels[ch].dad = (self.channels[ch].dad & 0x0000_FFFF) | ((val as u32) << 16),
            8 => self.channels[ch].count = val as u32,
            10 => self.channels[ch].write_control(val, ch),
            _ => {}
        }
    }

    pub fn read16(&self, addr: u32, open_bus: u16) -> u16 {
        let ch = ((addr - 0x040000B0) / 12) as usize;
        let reg = (addr - 0x040000B0) % 12;
        if ch >= 4 { return open_bus; }
        // SAD/DAD/count are write-only and read back as 0.
        if reg == 10 {
            self.channels[ch].read_control()
        } else {
            0
        }
    }
}

