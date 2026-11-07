//! MMIO Register Layout & Handlers for GBA (0x04000000 - 0x040003FE).

pub struct IoRegisters {
    // Interrupt control
    pub ime: bool,
    pub ie: u16,
    pub if_: u16,

    // Keypad
    pub keyinput: u16, // active-low (0 = pressed)
    pub keycnt: u16,

    // Waitstate control
    pub waitcnt: u16,

    // Power / System control
    pub postflg: u8,
    pub haltcnt: bool,
}

impl Default for IoRegisters {
    fn default() -> Self {
        Self::new()
    }
}

impl IoRegisters {
    pub fn new() -> Self {
        Self {
            ime: false,
            ie: 0,
            if_: 0,
            keyinput: 0x03FF, // all buttons released
            keycnt: 0,
            waitcnt: 0,
            postflg: 1, // post-boot
            haltcnt: false,
        }
    }

    #[inline(always)]
    pub fn irq_pending(&self) -> bool {
        self.ime && (self.ie & self.if_) != 0
    }
}

