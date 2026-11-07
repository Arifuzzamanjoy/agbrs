//! ARM7TDMI 32-bit RISC processor core.

pub mod registers;
pub mod alu;
pub mod arm;
pub mod thumb;

use registers::*;
use crate::bus::Bus;

pub struct Cpu {
    pub r: [u32; 16],
    pub cpsr: u32,
    pub mode: Mode,
    pub banked: BankedRegisters,
    pub pipe: [u32; 2],
    pub halted: bool,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            r: [0; 16],
            cpsr: MODE_SYS, // Start in System mode
            mode: Mode::System,
            banked: BankedRegisters::default(),
            pipe: [0; 2],
            halted: false,
        }
    }

    pub fn reset(&mut self, bus: &mut Bus) {
        self.mode = Mode::Supervisor;
        self.cpsr = MODE_SVC | FLAG_I | FLAG_F;
        self.r = [0; 16];
        self.r[13] = 0x03007FE0; // SP_svc
        self.banked = BankedRegisters::default();
        self.halted = false;
        self.r[15] = 0x00000000;
        self.flush_pipeline(bus);
    }

    #[inline(always)]
    pub fn check_cond(&self, cond: u32) -> bool {
        let n = (self.cpsr & FLAG_N) != 0;
        let z = (self.cpsr & FLAG_Z) != 0;
        let c = (self.cpsr & FLAG_C) != 0;
        let v = (self.cpsr & FLAG_V) != 0;

        match cond {
            0x0 => z,                   // EQ: Z == 1
            0x1 => !z,                  // NE: Z == 0
            0x2 => c,                   // CS / HS: C == 1
            0x3 => !c,                  // CC / LO: C == 0
            0x4 => n,                   // MI: N == 1
            0x5 => !n,                  // PL: N == 0
            0x6 => v,                   // VS: V == 1
            0x7 => !v,                  // VC: V == 0
            0x8 => c && !z,             // HI: C == 1 and Z == 0
            0x9 => !c || z,             // LS: C == 0 or Z == 1
            0xA => n == v,              // GE: N == V
            0xB => n != v,              // LT: N != V
            0xC => !z && (n == v),      // GT: Z == 0 and N == V
            0xD => z || (n != v),       // LE: Z == 1 or N != V
            0xE => true,                // AL: Always
            0xF => false,               // NV: Never
            _ => true,
        }
    }

    pub fn spsr_val(&self) -> u32 {
        match self.mode {
            Mode::Fiq => self.banked.spsr_fiq,
            Mode::Supervisor => self.banked.spsr_svc,
            Mode::Abort => self.banked.spsr_abt,
            Mode::Irq => self.banked.spsr_irq,
            Mode::Undefined => self.banked.spsr_und,
            _ => self.cpsr,
        }
    }

    pub fn set_spsr_val(&mut self, val: u32, mask: u32) {
        match self.mode {
            Mode::Fiq => self.banked.spsr_fiq = (self.banked.spsr_fiq & !mask) | (val & mask),
            Mode::Supervisor => self.banked.spsr_svc = (self.banked.spsr_svc & !mask) | (val & mask),
            Mode::Abort => self.banked.spsr_abt = (self.banked.spsr_abt & !mask) | (val & mask),
            Mode::Irq => self.banked.spsr_irq = (self.banked.spsr_irq & !mask) | (val & mask),
            Mode::Undefined => self.banked.spsr_und = (self.banked.spsr_und & !mask) | (val & mask),
            _ => {}
        }
    }

    pub fn swap_mode(&mut self, new_mode: Mode) {
        if self.mode == new_mode {
            return;
        }

        // Save current mode registers
        match self.mode {
            Mode::User | Mode::System => {
                self.banked.r8_usr = self.r[8];
                self.banked.r9_usr = self.r[9];
                self.banked.r10_usr = self.r[10];
                self.banked.r11_usr = self.r[11];
                self.banked.r12_usr = self.r[12];
                self.banked.r13_usr = self.r[13];
                self.banked.r14_usr = self.r[14];
            }
            Mode::Fiq => {
                self.banked.r8_fiq = self.r[8];
                self.banked.r9_fiq = self.r[9];
                self.banked.r10_fiq = self.r[10];
                self.banked.r11_fiq = self.r[11];
                self.banked.r12_fiq = self.r[12];
                self.banked.r13_fiq = self.r[13];
                self.banked.r14_fiq = self.r[14];
            }
            Mode::Supervisor => {
                self.banked.r8_usr = self.r[8];
                self.banked.r9_usr = self.r[9];
                self.banked.r10_usr = self.r[10];
                self.banked.r11_usr = self.r[11];
                self.banked.r12_usr = self.r[12];
                self.banked.r13_svc = self.r[13];
                self.banked.r14_svc = self.r[14];
            }
            Mode::Abort => {
                self.banked.r8_usr = self.r[8];
                self.banked.r9_usr = self.r[9];
                self.banked.r10_usr = self.r[10];
                self.banked.r11_usr = self.r[11];
                self.banked.r12_usr = self.r[12];
                self.banked.r13_abt = self.r[13];
                self.banked.r14_abt = self.r[14];
            }
            Mode::Irq => {
                self.banked.r8_usr = self.r[8];
                self.banked.r9_usr = self.r[9];
                self.banked.r10_usr = self.r[10];
                self.banked.r11_usr = self.r[11];
                self.banked.r12_usr = self.r[12];
                self.banked.r13_irq = self.r[13];
                self.banked.r14_irq = self.r[14];
            }
            Mode::Undefined => {
                self.banked.r8_usr = self.r[8];
                self.banked.r9_usr = self.r[9];
                self.banked.r10_usr = self.r[10];
                self.banked.r11_usr = self.r[11];
                self.banked.r12_usr = self.r[12];
                self.banked.r13_und = self.r[13];
                self.banked.r14_und = self.r[14];
            }
        }

        // Restore new mode registers
        match new_mode {
            Mode::User | Mode::System => {
                self.r[8] = self.banked.r8_usr;
                self.r[9] = self.banked.r9_usr;
                self.r[10] = self.banked.r10_usr;
                self.r[11] = self.banked.r11_usr;
                self.r[12] = self.banked.r12_usr;
                self.r[13] = self.banked.r13_usr;
                self.r[14] = self.banked.r14_usr;
            }
            Mode::Fiq => {
                self.r[8] = self.banked.r8_fiq;
                self.r[9] = self.banked.r9_fiq;
                self.r[10] = self.banked.r10_fiq;
                self.r[11] = self.banked.r11_fiq;
                self.r[12] = self.banked.r12_fiq;
                self.r[13] = self.banked.r13_fiq;
                self.r[14] = self.banked.r14_fiq;
            }
            Mode::Supervisor => {
                self.r[8] = self.banked.r8_usr;
                self.r[9] = self.banked.r9_usr;
                self.r[10] = self.banked.r10_usr;
                self.r[11] = self.banked.r11_usr;
                self.r[12] = self.banked.r12_usr;
                self.r[13] = self.banked.r13_svc;
                self.r[14] = self.banked.r14_svc;
            }
            Mode::Abort => {
                self.r[8] = self.banked.r8_usr;
                self.r[9] = self.banked.r9_usr;
                self.r[10] = self.banked.r10_usr;
                self.r[11] = self.banked.r11_usr;
                self.r[12] = self.banked.r12_usr;
                self.r[13] = self.banked.r13_abt;
                self.r[14] = self.banked.r14_abt;
            }
            Mode::Irq => {
                self.r[8] = self.banked.r8_usr;
                self.r[9] = self.banked.r9_usr;
                self.r[10] = self.banked.r10_usr;
                self.r[11] = self.banked.r11_usr;
                self.r[12] = self.banked.r12_usr;
                self.r[13] = self.banked.r13_irq;
                self.r[14] = self.banked.r14_irq;
            }
            Mode::Undefined => {
                self.r[8] = self.banked.r8_usr;
                self.r[9] = self.banked.r9_usr;
                self.r[10] = self.banked.r10_usr;
                self.r[11] = self.banked.r11_usr;
                self.r[12] = self.banked.r12_usr;
                self.r[13] = self.banked.r13_und;
                self.r[14] = self.banked.r14_und;
            }
        }

        self.mode = new_mode;
    }

    pub fn restore_spsr(&mut self) {
        let spsr = self.spsr_val();
        let new_mode = Mode::from_bits(spsr);
        self.swap_mode(new_mode);
        self.cpsr = spsr;
    }

    pub fn swap_user_bank(&mut self) {
        if self.mode == Mode::User || self.mode == Mode::System {
            return;
        }
        if self.mode == Mode::Fiq {
            core::mem::swap(&mut self.r[8], &mut self.banked.r8_usr);
            core::mem::swap(&mut self.r[9], &mut self.banked.r9_usr);
            core::mem::swap(&mut self.r[10], &mut self.banked.r10_usr);
            core::mem::swap(&mut self.r[11], &mut self.banked.r11_usr);
            core::mem::swap(&mut self.r[12], &mut self.banked.r12_usr);
        }
        core::mem::swap(&mut self.r[13], &mut self.banked.r13_usr);
        core::mem::swap(&mut self.r[14], &mut self.banked.r14_usr);
    }

    pub fn flush_pipeline(&mut self, bus: &mut Bus) {
        if self.cpsr & FLAG_T != 0 {
            self.r[15] &= !1;
            let target = self.r[15];
            self.pipe[0] = bus.read16(target, target) as u32;
            self.pipe[1] = bus.read16(target.wrapping_add(2), target.wrapping_add(2)) as u32;
            self.r[15] = target.wrapping_add(4);
        } else {
            self.r[15] &= !3;
            let target = self.r[15];
            self.pipe[0] = bus.read32(target, target);
            self.pipe[1] = bus.read32(target.wrapping_add(4), target.wrapping_add(4));
            self.r[15] = target.wrapping_add(8);
        }
    }

    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        // Wakeup from halt on any pending enabled interrupt
        if (bus.io.if_ & bus.io.ie) != 0 {
            self.halted = false;
        }

        if self.halted {
            return 1;
        }

        // Check for IRQ exception
        if (self.cpsr & FLAG_I) == 0 && bus.io.irq_pending() {
            let lr = if self.cpsr & FLAG_T != 0 {
                self.r[15]
            } else {
                self.r[15].wrapping_sub(4)
            };
            self.banked.r14_irq = lr;
            self.banked.spsr_irq = self.cpsr;
            self.swap_mode(Mode::Irq);
            self.cpsr |= FLAG_I;
            self.cpsr &= !FLAG_T;
            self.r[15] = 0x00000018;
            self.flush_pipeline(bus);
            return 3;
        }

        if self.cpsr & FLAG_T != 0 {
            let op = self.pipe[0] as u16;
            let old_r15 = self.r[15];
            let cycles = self.exec_thumb(op, bus);
            if self.r[15] == old_r15 {
                self.pipe[0] = self.pipe[1];
                let fetch_addr = self.r[15];
                self.pipe[1] = bus.read16(fetch_addr & !1, fetch_addr) as u32;
                self.r[15] = self.r[15].wrapping_add(2);
            }
            cycles
        } else {
            let op = self.pipe[0];
            let old_r15 = self.r[15];
            let cycles = self.exec_arm(op, bus);
            if self.r[15] == old_r15 {
                self.pipe[0] = self.pipe[1];
                let fetch_addr = self.r[15];
                self.pipe[1] = bus.read32(fetch_addr & !3, fetch_addr);
                self.r[15] = self.r[15].wrapping_add(4);
            }
            cycles
        }
    }
}
