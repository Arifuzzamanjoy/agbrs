//! ARMv4T ALU, Barrel Shifter, and Multiply Operations.

use super::registers::{FLAG_C, FLAG_N, FLAG_V, FLAG_Z};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShiftType {
    Lsl = 0,
    Lsr = 1,
    Asr = 2,
    Ror = 3,
}

#[inline(always)]
pub fn shift_imm(shift_type: ShiftType, rm: u32, amt: u32, carry_in: bool) -> (u32, bool) {
    match shift_type {
        ShiftType::Lsl => {
            if amt == 0 {
                (rm, carry_in)
            } else {
                let res = rm << amt;
                let carry_out = (rm >> (32 - amt)) & 1 != 0;
                (res, carry_out)
            }
        }
        ShiftType::Lsr => {
            if amt == 0 {
                // LSR #0 encodes LSR #32
                (0, (rm >> 31) & 1 != 0)
            } else {
                let res = rm >> amt;
                let carry_out = (rm >> (amt - 1)) & 1 != 0;
                (res, carry_out)
            }
        }
        ShiftType::Asr => {
            if amt == 0 {
                // ASR #0 encodes ASR #32
                let sign = (rm >> 31) & 1;
                let res = if sign != 0 { 0xFFFF_FFFF } else { 0 };
                (res, sign != 0)
            } else {
                let res = ((rm as i32) >> amt) as u32;
                let carry_out = (rm >> (amt - 1)) & 1 != 0;
                (res, carry_out)
            }
        }
        ShiftType::Ror => {
            if amt == 0 {
                // ROR #0 encodes RRX #1
                let res = ((carry_in as u32) << 31) | (rm >> 1);
                let carry_out = rm & 1 != 0;
                (res, carry_out)
            } else {
                let res = rm.rotate_right(amt);
                let carry_out = (rm >> (amt - 1)) & 1 != 0;
                (res, carry_out)
            }
        }
    }
}

#[inline(always)]
pub fn shift_reg(shift_type: ShiftType, rm: u32, rs: u32, carry_in: bool) -> (u32, bool) {
    let amt = rs & 0xFF;
    if amt == 0 {
        return (rm, carry_in);
    }

    match shift_type {
        ShiftType::Lsl => {
            if amt < 32 {
                let res = rm << amt;
                let carry_out = (rm >> (32 - amt)) & 1 != 0;
                (res, carry_out)
            } else if amt == 32 {
                (0, rm & 1 != 0)
            } else {
                (0, false)
            }
        }
        ShiftType::Lsr => {
            if amt < 32 {
                let res = rm >> amt;
                let carry_out = (rm >> (amt - 1)) & 1 != 0;
                (res, carry_out)
            } else if amt == 32 {
                (0, (rm >> 31) & 1 != 0)
            } else {
                (0, false)
            }
        }
        ShiftType::Asr => {
            if amt < 32 {
                let res = ((rm as i32) >> amt) as u32;
                let carry_out = (rm >> (amt - 1)) & 1 != 0;
                (res, carry_out)
            } else {
                let sign = (rm >> 31) & 1 != 0;
                (if sign { 0xFFFF_FFFF } else { 0 }, sign)
            }
        }
        ShiftType::Ror => {
            let eff_amt = amt & 0x1F;
            if eff_amt == 0 {
                (rm, (rm >> 31) & 1 != 0)
            } else {
                let res = rm.rotate_right(eff_amt);
                let carry_out = (rm >> (eff_amt - 1)) & 1 != 0;
                (res, carry_out)
            }
        }
    }
}

#[inline(always)]
pub fn add(a: u32, b: u32, c_in: u32) -> (u32, u32) {
    let res64 = (a as u64) + (b as u64) + (c_in as u64);
    let res = res64 as u32;

    let mut flags = 0u32;
    if res & 0x8000_0000 != 0 { flags |= FLAG_N; }
    if res == 0 { flags |= FLAG_Z; }
    if res64 > 0xFFFF_FFFF { flags |= FLAG_C; }
    if (!((a ^ b)) & (a ^ res)) & 0x8000_0000 != 0 { flags |= FLAG_V; }

    (res, flags)
}

#[inline(always)]
pub fn sub(a: u32, b: u32, c_in: u32) -> (u32, u32) {
    // In ARM subtraction, c_in=1 means no borrow, c_in=0 means borrow.
    let borrow = 1 - c_in;
    let res64 = (a as u64).wrapping_sub(b as u64).wrapping_sub(borrow as u64);
    let res = res64 as u32;

    let mut flags = 0u32;
    if res & 0x8000_0000 != 0 { flags |= FLAG_N; }
    if res == 0 { flags |= FLAG_Z; }
    // Carry flag is set if NO borrow occurred: a >= b + borrow
    if (a as u64) >= ((b as u64) + (borrow as u64)) { flags |= FLAG_C; }
    if (((a ^ b)) & (a ^ res)) & 0x8000_0000 != 0 { flags |= FLAG_V; }

    (res, flags)
}

#[inline(always)]
pub fn logic_flags(res: u32, carry: bool, old_cpsr: u32) -> u32 {
    let mut flags = old_cpsr & !(FLAG_N | FLAG_Z | FLAG_C);
    if res & 0x8000_0000 != 0 { flags |= FLAG_N; }
    if res == 0 { flags |= FLAG_Z; }
    if carry { flags |= FLAG_C; }
    flags
}

