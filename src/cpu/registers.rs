//! ARM7TDMI register file and execution modes.

pub const MODE_USR: u32 = 0x10;
pub const MODE_FIQ: u32 = 0x11;
pub const MODE_IRQ: u32 = 0x12;
pub const MODE_SVC: u32 = 0x13;
pub const MODE_ABT: u32 = 0x17;
pub const MODE_UND: u32 = 0x1B;
pub const MODE_SYS: u32 = 0x1F;

pub const FLAG_N: u32 = 1 << 31;
pub const FLAG_Z: u32 = 1 << 30;
pub const FLAG_C: u32 = 1 << 29;
pub const FLAG_V: u32 = 1 << 28;
pub const FLAG_I: u32 = 1 << 7;
pub const FLAG_F: u32 = 1 << 6;
pub const FLAG_T: u32 = 1 << 5;
pub const MODE_MASK: u32 = 0x1F;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    User = 0x10,
    Fiq = 0x11,
    Irq = 0x12,
    Supervisor = 0x13,
    Abort = 0x17,
    Undefined = 0x1B,
    System = 0x1F,
}

impl Mode {
    #[inline]
    pub fn from_bits(bits: u32) -> Self {
        match bits & MODE_MASK {
            MODE_USR => Mode::User,
            MODE_FIQ => Mode::Fiq,
            MODE_IRQ => Mode::Irq,
            MODE_SVC => Mode::Supervisor,
            MODE_ABT => Mode::Abort,
            MODE_UND => Mode::Undefined,
            MODE_SYS => Mode::System,
            _ => Mode::User, // Fallback safe
        }
    }
}

pub struct BankedRegisters {
    // User / System share R8-R14 in user bank
    pub r8_usr: u32,
    pub r9_usr: u32,
    pub r10_usr: u32,
    pub r11_usr: u32,
    pub r12_usr: u32,
    pub r13_usr: u32,
    pub r14_usr: u32,

    // FIQ banks R8-R14 and SPSR
    pub r8_fiq: u32,
    pub r9_fiq: u32,
    pub r10_fiq: u32,
    pub r11_fiq: u32,
    pub r12_fiq: u32,
    pub r13_fiq: u32,
    pub r14_fiq: u32,
    pub spsr_fiq: u32,

    // Supervisor (SVC)
    pub r13_svc: u32,
    pub r14_svc: u32,
    pub spsr_svc: u32,

    // Abort
    pub r13_abt: u32,
    pub r14_abt: u32,
    pub spsr_abt: u32,

    // IRQ
    pub r13_irq: u32,
    pub r14_irq: u32,
    pub spsr_irq: u32,

    // Undefined
    pub r13_und: u32,
    pub r14_und: u32,
    pub spsr_und: u32,
}

impl Default for BankedRegisters {
    fn default() -> Self {
        Self {
            r8_usr: 0, r9_usr: 0, r10_usr: 0, r11_usr: 0, r12_usr: 0, r13_usr: 0x03007F00, r14_usr: 0,
            r8_fiq: 0, r9_fiq: 0, r10_fiq: 0, r11_fiq: 0, r12_fiq: 0, r13_fiq: 0, r14_fiq: 0, spsr_fiq: 0,
            r13_svc: 0x03007FE0, r14_svc: 0, spsr_svc: 0,
            r13_abt: 0, r14_abt: 0, spsr_abt: 0,
            r13_irq: 0x03007FA0, r14_irq: 0, spsr_irq: 0,
            r13_und: 0, r14_und: 0, spsr_und: 0,
        }
    }
}

