//! ARM7TDMI 32-bit RISC processor core.

pub mod registers;
pub mod alu;
pub mod arm;
pub mod thumb;

use registers::*;
use crate::bus::Bus;

/// Cycles between the wake condition and resuming from HALT.
pub const HALT_EXIT_CYCLES: u32 = 3;
/// Instructions executed after a HALT wake-up before an IRQ is taken.
pub const HALT_WAKE_INSNS: u32 = 2;
/// Minimum cycles between the IRQ line rising and the instruction
/// boundary at which the IRQ is taken.
pub const IRQ_LATENCY_CYCLES: u64 = 9;
/// Instructions the CPU executes after a DMA before it can take an IRQ.
pub const DMA_IRQ_HOLD_INSNS: u32 = 2;
/// Instructions executed after the HALTCNT write before the CPU stops.
pub const HALT_DELAY_INSNS: u32 = 0;

pub struct Cpu {
    pub r: [u32; 16],
    pub cpsr: u32,
    pub mode: Mode,
    pub banked: BankedRegisters,
    pub pipe: [u32; 2],
    pub halted: bool,
    /// Address/width of the opcode fetch the current instruction would
    /// have made; charged (and discarded) when the pipeline is flushed.
    flush_fetch: (u32, u32),
    /// Set by `flush_pipeline`; tells `step` the instruction branched.
    /// (Comparing r15 before/after is not enough: e.g. `b .` or a BX to
    /// the following word refills to the same r15 value.)
    flushed: bool,
    /// Instructions still to run after a HALT wake-up before an IRQ can
    /// be taken.
    wake_insns: u32,
    /// Set by a HALT wake-up: the cycle latency is already satisfied.
    woke: bool,
    /// Internal cycles of the executing instruction not yet charged; they
    /// are charged before its opcode fetch (or before a refill).
    pending_internal: u32,
    /// Instructions left before a requested HALT engages.
    halt_countdown: u32,
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
            flush_fetch: (u32::MAX, 0),
            flushed: false,
            wake_insns: 0,
            woke: false,
            pending_internal: 0,
            halt_countdown: 0,
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
        self.flush_fetch = (u32::MAX, 0);
        self.flush_pipeline(bus);
        bus.take_cycles();
    }

    /// Direct boot (what the reference does with its boot screen
    /// skipped): the state the BIOS leaves before jumping to the cart.
    pub fn reset_skip_bios(&mut self, bus: &mut Bus) {
        self.reset(bus);
        self.banked.r13_svc = 0x0300_7FE0;
        self.banked.r13_irq = 0x0300_7FA0;
        self.swap_mode(Mode::System);
        self.cpsr = MODE_SYS;
        self.r[13] = 0x0300_7F00;
        self.r[15] = 0x0800_0000;
        self.flush_fetch = (u32::MAX, 0);
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
        self.flushed = true;
        // A branch costs one extra (discarded) cycle; the reference charges
        // it as a sequential access at the destination, in the new state's
        // width (lockstep-verified: Thumb ROM 3+5+3, IWRAM 1+1+1).
        let internal = core::mem::take(&mut self.pending_internal);
        bus.idle(internal);
        bus.prefetch_branch_reset();
        let (_pc, w) = core::mem::replace(&mut self.flush_fetch, (u32::MAX, 0));
        if w != 0 {
            let width = if self.cpsr & FLAG_T != 0 { 2 } else { 4 };
            bus.branch_dummy_cycle(self.r[15], width);
        }
        bus.break_sequence();
        if self.cpsr & FLAG_T != 0 {
            self.r[15] &= !1;
            let target = self.r[15];
            self.pipe[0] = bus.fetch16(target) as u32;
            self.pipe[1] = bus.fetch16(target.wrapping_add(2)) as u32;
            self.r[15] = target.wrapping_add(4);
        } else {
            self.r[15] &= !3;
            let target = self.r[15];
            self.pipe[0] = bus.fetch32(target);
            self.pipe[1] = bus.fetch32(target.wrapping_add(4));
            self.r[15] = target.wrapping_add(8);
        }
    }

    /// Multiplier early-termination cycles (m) for a Rs operand.
    #[inline(always)]
    fn mul_cycles(rs: u32, signed: bool) -> u32 {
        let check = |mask: u32| {
            let v = rs & mask;
            v == 0 || (signed && v == mask)
        };
        if check(0xFFFF_FF00) {
            1
        } else if check(0xFFFF_0000) {
            2
        } else if check(0xFF00_0000) {
            3
        } else {
            4
        }
    }

    /// Internal (I) cycles of an ARM instruction, per GBATEK timings.
    /// Memory access cycles are charged by the bus itself.
    fn arm_internal_cycles(&self, op: u32) -> u32 {
        let bits27_20 = (op >> 20) & 0xFF;
        let bits7_4 = (op >> 4) & 0xF;
        if (op & 0x0FFF_FFF0) == 0x012F_FF10 || (bits27_20 >> 5) == 0x5 || bits27_20 >> 4 == 0xF {
            return 0; // BX / B / SWI
        }
        if (bits27_20 >> 5) == 0x4 {
            return if op & (1 << 20) != 0 { 1 } else { 0 }; // LDM: +1I
        }
        if bits7_4 == 0x9 && (bits27_20 & 0xFB) == 0x10 {
            return 1; // SWP
        }
        if bits7_4 == 0x9 && (bits27_20 & 0xFC) == 0x00 {
            let rs = self.r[((op >> 8) & 0xF) as usize];
            return Self::mul_cycles(rs, true) + ((op >> 21) & 1); // MUL/MLA
        }
        if bits7_4 == 0x9 && (bits27_20 & 0xF8) == 0x08 {
            let rs = self.r[((op >> 8) & 0xF) as usize];
            let signed = op & (1 << 22) != 0;
            return Self::mul_cycles(rs, signed) + 1 + ((op >> 21) & 1); // UMULL/SMULL/..LAL
        }
        if (bits7_4 & 0x9) == 0x9 && (bits7_4 & 0x6) != 0 && (bits27_20 & 0xE0) == 0x00 {
            return if op & (1 << 20) != 0 { 1 } else { 0 }; // LDRH/LDRSB/LDRSH
        }
        if (op >> 26) & 3 == 1 {
            return if op & (1 << 20) != 0 { 1 } else { 0 }; // LDR/LDRB
        }
        if (op >> 26) & 3 == 0 && op & (1 << 25) == 0 && op & (1 << 4) != 0 {
            return 1; // data processing with register-specified shift
        }
        0
    }

    /// Internal (I) cycles of a Thumb instruction.
    fn thumb_internal_cycles(&self, op: u16) -> u32 {
        let op = op as u32;
        if op >> 10 == 0x10 {
            // Format 4: ALU operations (010000xx). Format 5 (hi-reg/BX,
            // 010001xx) shares op>>11 and must not land here.
            return match (op >> 6) & 0xF {
                0x2 | 0x3 | 0x4 | 0x7 => 1, // LSL/LSR/ASR/ROR by register
                0xD => Self::mul_cycles(self.r[(op & 7) as usize], true), // MUL
                _ => 0,
            };
        }
        match op >> 11 {
            0x09 => 1, // LDR PC-relative
            0x0A | 0x0B => {
                // Formats 7/8: register offset; loads are L=1 or sign/halfword loads
                let opc = (op >> 9) & 7;
                // opc 0..=2 are STR/STRB/STRH; the rest are loads
                if opc >= 3 { 1 } else { 0 }
            }
            0x0C..=0x11 => if op & (1 << 11) != 0 { 1 } else { 0 }, // imm-offset LDR/LDRB/LDRH
            0x12 | 0x13 => if op & (1 << 11) != 0 { 1 } else { 0 }, // SP-relative
            0x16 | 0x17 => {
                // Format 14: PUSH/POP
                // POP: bits 11..9 = 1,1,0
                if (op >> 9) & 7 == 0b110 { 1 } else { 0 }
            }
            0x19 => 1, // LDMIA
            _ => 0,
        }
    }

    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        // Wake from halt on any pending enabled interrupt (IME ignored).
        // The reference resumes a few cycles after the line rises
        // (lockstep-verified).
        if self.halted && (bus.io.if_ & bus.io.ie) != 0 {
            self.halted = false;
            bus.idle(HALT_EXIT_CYCLES);
            // The reference runs two more instructions before entering
            // the handler after a wake-up.
            self.wake_insns = HALT_WAKE_INSNS;
            self.woke = true;
        }

        if self.halted {
            // Cycles drained here include any DMA that ran meanwhile.
            // Skip ahead to the next PPU/timer event.
            let pending = bus.take_cycles();
            return pending.max(bus.cycles_to_next_event());
        }

        // IRQ latency (calibrated by lockstep vs the reference): the IRQ
        // is taken at the first instruction boundary at least
        // IRQ_LATENCY_CYCLES after the line rose. After waking from HALT
        // two more instructions run first.
        if core::mem::take(&mut bus.dma_ran) {
            self.wake_insns = self.wake_insns.max(DMA_IRQ_HOLD_INSNS);
        }
        let take = self.wake_insns == 0
            && match bus.irq_since {
                Some(t) => self.woke || t + IRQ_LATENCY_CYCLES <= bus.now,
                None => false,
            };
        if bus.irq_since.is_none() {
            self.woke = false;
        }
        if (self.cpsr & FLAG_I) == 0 && take {
            self.woke = false;
            let lr = if self.cpsr & FLAG_T != 0 {
                self.r[15]
            } else {
                self.r[15].wrapping_sub(4)
            };
            self.banked.r14_irq = lr;
            self.banked.spsr_irq = self.cpsr;
            self.swap_mode(Mode::Irq);
            self.flush_fetch = (self.r[15], if self.cpsr & FLAG_T != 0 { 2 } else { 4 });
            self.cpsr |= FLAG_I;
            self.cpsr &= !FLAG_T;
            self.r[15] = 0x00000018;
            self.flush_pipeline(bus);
            return bus.take_cycles();
        }

        if self.cpsr & FLAG_T != 0 {
            let op = self.pipe[0] as u16;
            let old_r15 = self.r[15];
            let internal = self.thumb_internal_cycles(op);
            self.flush_fetch = (old_r15, 2);
            self.flushed = false;
            self.pending_internal = internal;
            self.exec_thumb(op, bus);
            if !self.flushed {
                self.flush_fetch = (u32::MAX, 0);
                bus.idle(core::mem::take(&mut self.pending_internal));
                self.pipe[0] = self.pipe[1];
                let fetch_addr = self.r[15];
                self.pipe[1] = bus.fetch16(fetch_addr & !1) as u32;
                self.r[15] = self.r[15].wrapping_add(2);
            }
        } else {
            let op = self.pipe[0];
            let old_r15 = self.r[15];
            let cond = op >> 28;
            let internal = if cond == 0xE || self.check_cond(cond) {
                self.arm_internal_cycles(op)
            } else {
                0
            };
            self.flush_fetch = (old_r15, 4);
            self.flushed = false;
            self.pending_internal = internal;
            self.exec_arm(op, bus);
            if !self.flushed {
                self.flush_fetch = (u32::MAX, 0);
                bus.idle(core::mem::take(&mut self.pending_internal));
                self.pipe[0] = self.pipe[1];
                let fetch_addr = self.r[15];
                self.pipe[1] = bus.fetch32(fetch_addr & !3);
                self.r[15] = self.r[15].wrapping_add(4);
            }
        }

        if self.wake_insns > 0 {
            self.wake_insns -= 1;
        }
        // HALT engages after HALT_DELAY_INSNS more instructions (0 matches
        // the reference under lockstep; kept configurable).
        if self.halt_countdown > 0 {
            self.halt_countdown -= 1;
            if self.halt_countdown == 0 {
                self.halted = true;
            }
        }
        if bus.halt_request {
            bus.halt_request = false;
            if HALT_DELAY_INSNS == 0 {
                self.halted = true;
            } else {
                self.halt_countdown = HALT_DELAY_INSNS;
            }
        }
        bus.take_cycles().max(1)
    }
}
