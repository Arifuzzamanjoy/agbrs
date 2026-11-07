//! ARM (32-bit) instruction decoder and executor for ARMv4T.

use super::registers::*;
use super::alu::*;
use super::Cpu;
use crate::bus::Bus;

impl Cpu {
    pub fn exec_arm(&mut self, op: u32, bus: &mut Bus) -> u32 {
        let cond = (op >> 28) & 0xF;
        if cond != 0xE && !self.check_cond(cond) {
            return 1;
        }

        let bits27_20 = (op >> 20) & 0xFF;
        let bits7_4 = (op >> 4) & 0xF;

        // ---- Branch and Exchange (BX) ----
        // 0001_0010_xxxx_xxxx_xxxx_0001_xxxx
        if (op & 0x0FFF_FFF0) == 0x012F_FF10 {
            let rm = (op & 0xF) as usize;
            let addr = self.r[rm];
            if addr & 1 != 0 {
                self.cpsr |= FLAG_T;
            } else {
                self.cpsr &= !FLAG_T;
            }
            self.r[15] = addr & !1;
            self.flush_pipeline(bus);
            return 3;
        }

        // ---- Software Interrupt (SWI) ----
        // 1111_xxxx_xxxx_xxxx_xxxx_xxxx_xxxx
        if bits27_20 >> 4 == 0xF {
            return self.arm_swi(bus);
        }

        // ---- Branch / Branch with Link (B, BL) ----
        // 101x_xxxx_xxxx_xxxx_xxxx_xxxx_xxxx
        if (bits27_20 >> 5) == 0x5 {
            let link = op & (1 << 24) != 0;
            if link {
                // Return address: instruction after BL (PC is current_instr + 8, so PC - 4)
                self.r[14] = self.r[15].wrapping_sub(4);
            }
            let offset = ((op & 0x00FF_FFFF) as i32) << 8 >> 6; // sign extend 24-bit * 4
            self.r[15] = (self.r[15] as i32).wrapping_add(offset) as u32;
            self.flush_pipeline(bus);
            return 3;
        }

        // ---- Block Data Transfer (LDM/STM) ----
        // 100x_xxxx_xxxx_xxxx_xxxx_xxxx_xxxx
        if (bits27_20 >> 5) == 0x4 {
            return self.arm_block_transfer(op, bus);
        }

        // ---- Single Data Swap (SWP / SWPB) ----
        // 0001_0x00_xxxx_xxxx_xxxx_1001_xxxx
        if (bits27_20 & 0xFB) == 0x10 && bits7_4 == 0x9 {
            return self.arm_swap(op, bus);
        }

        // ---- Multiply / Multiply-Long ----
        if bits7_4 == 0x9 {
            if (bits27_20 & 0xFC) == 0x00 {
                return self.arm_multiply(op);
            }
            if (bits27_20 & 0xF8) == 0x08 {
                return self.arm_multiply_long(op);
            }
        }

        // ---- Halfword / Signed Transfer ----
        // bits 7:4 = 1xx1 (except 1001), bit 25 = 0
        if (bits7_4 & 0x9) == 0x9 && (bits7_4 & 0x6) != 0 && (bits27_20 & 0xE0) == 0x00 {
            return self.arm_halfword_transfer(op, bus);
        }

        // ---- MRS (read CPSR/SPSR) ----
        if (op & 0x0FBF_0FFF) == 0x010F_0000 {
            return self.arm_mrs(op);
        }

        // ---- MSR (write CPSR/SPSR) ----
        if (op & 0x0FB0_FFF0) == 0x0120_F000 || (op & 0x0FB0_F000) == 0x0320_F000 {
            return self.arm_msr(op);
        }

        // ---- Single Data Transfer (LDR/STR) ----
        // 01xx_xxxx_xxxx_xxxx_xxxx_xxxx_xxxx
        if (op >> 26) & 3 == 1 {
            return self.arm_single_transfer(op, bus);
        }

        // ---- Data Processing ----
        // 00xx_xxxx_xxxx_xxxx_xxxx_xxxx_xxxx
        if (op >> 26) & 3 == 0 {
            return self.arm_data_processing(op, bus);
        }

        1 // Unhandled opcode / NOP
    }

    fn arm_swi(&mut self, bus: &mut Bus) -> u32 {
        self.banked.r14_svc = self.r[15].wrapping_sub(4);
        self.banked.spsr_svc = self.cpsr;
        self.swap_mode(Mode::Supervisor);
        self.cpsr |= FLAG_I;
        self.cpsr &= !FLAG_T;
        self.r[15] = 0x0000_0008;
        self.flush_pipeline(bus);
        3
    }

    #[allow(unused_assignments)]
    fn arm_data_processing(&mut self, op: u32, bus: &mut Bus) -> u32 {
        let i_bit = op & (1 << 25) != 0;
        let s_bit = op & (1 << 20) != 0;
        let opcode = (op >> 21) & 0xF;
        let rn = ((op >> 16) & 0xF) as usize;
        let rd = ((op >> 12) & 0xF) as usize;

        let carry_in = (self.cpsr & FLAG_C) != 0;
        let (op2, shift_carry) = if i_bit {
            let imm = op & 0xFF;
            let rot = ((op >> 8) & 0xF) * 2;
            if rot == 0 {
                (imm, carry_in)
            } else {
                let v = imm.rotate_right(rot);
                (v, (v & 0x8000_0000) != 0)
            }
        } else {
            let shift_type = match (op >> 5) & 3 {
                0 => ShiftType::Lsl,
                1 => ShiftType::Lsr,
                2 => ShiftType::Asr,
                _ => ShiftType::Ror,
            };
            let is_reg_shift = (op & (1 << 4)) != 0;
            let rm = (op & 0xF) as usize;
            let rm_val = if is_reg_shift && rm == 15 {
                self.r[15].wrapping_add(4)
            } else {
                self.r[rm]
            };
            if is_reg_shift {
                let rs = ((op >> 8) & 0xF) as usize;
                let rs_val = if rs == 15 {
                    self.r[15].wrapping_add(4)
                } else {
                    self.r[rs]
                };
                shift_reg(shift_type, rm_val, rs_val, carry_in)
            } else {
                let amt = (op >> 7) & 0x1F;
                shift_imm(shift_type, rm_val, amt, carry_in)
            }
        };

        // When reg-shift is used, reading PC as Rn returns PC + 12 on hardware
        let reg_shift = !i_bit && (op & (1 << 4)) != 0;
        let a = if reg_shift && rn == 15 {
            self.r[15].wrapping_add(4)
        } else {
            self.r[rn]
        };

        let mut res = 0u32;
        let mut flags = self.cpsr;
        let mut write_rd = true;

        match opcode {
            0x0 => { // AND
                res = a & op2;
                if s_bit { flags = logic_flags(res, shift_carry, self.cpsr); }
            }
            0x1 => { // EOR
                res = a ^ op2;
                if s_bit { flags = logic_flags(res, shift_carry, self.cpsr); }
            }
            0x2 => { // SUB
                let (r, f) = sub(a, op2, 1);
                res = r;
                if s_bit { flags = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | f; }
            }
            0x3 => { // RSB
                let (r, f) = sub(op2, a, 1);
                res = r;
                if s_bit { flags = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | f; }
            }
            0x4 => { // ADD
                let (r, f) = add(a, op2, 0);
                res = r;
                if s_bit { flags = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | f; }
            }
            0x5 => { // ADC
                let c = if (self.cpsr & FLAG_C) != 0 { 1 } else { 0 };
                let (r, f) = add(a, op2, c);
                res = r;
                if s_bit { flags = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | f; }
            }
            0x6 => { // SBC
                let c = if (self.cpsr & FLAG_C) != 0 { 1 } else { 0 };
                let (r, f) = sub(a, op2, c);
                res = r;
                if s_bit { flags = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | f; }
            }
            0x7 => { // RSC
                let c = if (self.cpsr & FLAG_C) != 0 { 1 } else { 0 };
                let (r, f) = sub(op2, a, c);
                res = r;
                if s_bit { flags = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | f; }
            }
            0x8 => { // TST
                res = a & op2;
                flags = logic_flags(res, shift_carry, self.cpsr);
                write_rd = false;
            }
            0x9 => { // TEQ
                res = a ^ op2;
                flags = logic_flags(res, shift_carry, self.cpsr);
                write_rd = false;
            }
            0xA => { // CMP
                let (r, f) = sub(a, op2, 1);
                res = r;
                flags = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | f;
                write_rd = false;
            }
            0xB => { // CMN
                let (r, f) = add(a, op2, 0);
                res = r;
                flags = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | f;
                write_rd = false;
            }
            0xC => { // ORR
                res = a | op2;
                if s_bit { flags = logic_flags(res, shift_carry, self.cpsr); }
            }
            0xD => { // MOV
                res = op2;
                if s_bit { flags = logic_flags(res, shift_carry, self.cpsr); }
            }
            0xE => { // BIC
                res = a & !op2;
                if s_bit { flags = logic_flags(res, shift_carry, self.cpsr); }
            }
            0xF => { // MVN
                res = !op2;
                if s_bit { flags = logic_flags(res, shift_carry, self.cpsr); }
            }
            _ => {}
        }

        if write_rd {
            self.r[rd] = res;
        }

        if s_bit {
            if rd == 15 {
                // If Rd=15, SPSR of current mode is restored into CPSR
                self.restore_spsr();
            } else {
                self.cpsr = flags;
            }
        }

        if rd == 15 && write_rd {
            self.flush_pipeline(bus);
            3
        } else {
            1
        }
    }

    fn arm_single_transfer(&mut self, op: u32, bus: &mut Bus) -> u32 {
        let i_bit = op & (1 << 25) != 0;
        let p_bit = op & (1 << 24) != 0;
        let u_bit = op & (1 << 23) != 0;
        let b_bit = op & (1 << 22) != 0;
        let w_bit = op & (1 << 21) != 0;
        let l_bit = op & (1 << 20) != 0;

        let rn = ((op >> 16) & 0xF) as usize;
        let rd = ((op >> 12) & 0xF) as usize;

        let offset = if !i_bit {
            op & 0xFFF
        } else {
            let rm = (op & 0xF) as usize;
            let rm_val = self.r[rm];
            let shift_type = match (op >> 5) & 3 {
                0 => ShiftType::Lsl,
                1 => ShiftType::Lsr,
                2 => ShiftType::Asr,
                _ => ShiftType::Ror,
            };
            let amt = (op >> 7) & 0x1F;
            let carry_in = (self.cpsr & FLAG_C) != 0;
            let (v, _) = shift_imm(shift_type, rm_val, amt, carry_in);
            v
        };

        let base = self.r[rn];
        let addr = if u_bit {
            base.wrapping_add(offset)
        } else {
            base.wrapping_sub(offset)
        };

        let target_addr = if p_bit { addr } else { base };

        if l_bit {
            // Load
            let data = if b_bit {
                bus.read8(target_addr, self.r[15]) as u32
            } else {
                let w = bus.read32(target_addr & !3, self.r[15]);
                // ARM unaligned LDR rotates right by (addr & 3) * 8
                w.rotate_right((target_addr & 3) * 8)
            };
            self.r[rd] = data;

            // Writeback
            if !p_bit || w_bit {
                if rd != rn {
                    self.r[rn] = addr;
                }
            }

            if rd == 15 {
                self.flush_pipeline(bus);
                3
            } else {
                2
            }
        } else {
            // Store
            let val = if rd == 15 {
                self.r[15].wrapping_add(4)
            } else {
                self.r[rd]
            };

            if b_bit {
                bus.write8(target_addr, val as u8);
            } else {
                bus.write32(target_addr & !3, val);
            }

            if !p_bit || w_bit {
                self.r[rn] = addr;
            }
            2
        }
    }

    fn arm_halfword_transfer(&mut self, op: u32, bus: &mut Bus) -> u32 {
        let p_bit = op & (1 << 24) != 0;
        let u_bit = op & (1 << 23) != 0;
        let i_bit = op & (1 << 22) != 0;
        let w_bit = op & (1 << 21) != 0;
        let l_bit = op & (1 << 20) != 0;

        let rn = ((op >> 16) & 0xF) as usize;
        let rd = ((op >> 12) & 0xF) as usize;
        let opcode = (op >> 5) & 3;

        let offset = if i_bit {
            ((op >> 4) & 0xF0) | (op & 0xF)
        } else {
            let rm = (op & 0xF) as usize;
            self.r[rm]
        };

        let base = self.r[rn];
        let addr = if u_bit {
            base.wrapping_add(offset)
        } else {
            base.wrapping_sub(offset)
        };

        let target_addr = if p_bit { addr } else { base };

        if l_bit {
            // Load
            let data = match opcode {
                1 => { // LDRH
                    let h = bus.read16(target_addr & !1, self.r[15]) as u32;
                    if target_addr & 1 != 0 {
                        // Unaligned LDRH on ARM7TDMI rotates right by 8 bits
                        h.rotate_right(8)
                    } else {
                        h
                    }
                }
                2 => { // LDRSB
                    (bus.read8(target_addr, self.r[15]) as i8) as i32 as u32
                }
                3 => { // LDRSH
                    if target_addr & 1 != 0 {
                        // Unaligned LDRSH on ARM7TDMI reads signed byte!
                        (bus.read8(target_addr, self.r[15]) as i8) as i32 as u32
                    } else {
                        (bus.read16(target_addr, self.r[15]) as i16) as i32 as u32
                    }
                }
                _ => 0,
            };

            self.r[rd] = data;

            if !p_bit || w_bit {
                if rd != rn {
                    self.r[rn] = addr;
                }
            }

            if rd == 15 {
                self.flush_pipeline(bus);
                3
            } else {
                2
            }
        } else {
            // Store
            let val = if rd == 15 {
                self.r[15].wrapping_add(4)
            } else {
                self.r[rd]
            };

            match opcode {
                1 => bus.write16(target_addr & !1, val as u16),
                _ => {}
            }

            if !p_bit || w_bit {
                self.r[rn] = addr;
            }
            2
        }
    }

    fn arm_block_transfer(&mut self, op: u32, bus: &mut Bus) -> u32 {
        let p_bit = op & (1 << 24) != 0;
        let u_bit = op & (1 << 23) != 0;
        let s_bit = op & (1 << 22) != 0;
        let w_bit = op & (1 << 21) != 0;
        let l_bit = op & (1 << 20) != 0;

        let rn = ((op >> 16) & 0xF) as usize;
        let rlist = op & 0xFFFF;

        let count = rlist.count_ones();
        let base = self.r[rn];

        let lowest_addr = match (p_bit, u_bit) {
            (false, true)  => base,                                          // IA: Increment After
            (true,  true)  => base.wrapping_add(4),                          // IB: Increment Before
            (false, false) => base.wrapping_sub(count * 4).wrapping_add(4), // DA: Decrement After
            (true,  false) => base.wrapping_sub(count * 4),                  // DB: Decrement Before
        };

        if count == 0 {
            // ARM7TDMI quirk: empty register list loads/stores R15 and adjusts base by 64
            let addr = if u_bit { base } else { base.wrapping_sub(64) };
            let target = if p_bit == u_bit { addr.wrapping_add(4) } else { addr };
            if l_bit {
                self.r[15] = bus.read32(target & !3, self.r[15]);
                self.flush_pipeline(bus);
            } else {
                bus.write32(target & !3, self.r[15].wrapping_add(4));
            }
            if w_bit {
                self.r[rn] = if u_bit { base.wrapping_add(64) } else { base.wrapping_sub(64) };
            }
            return 3;
        }

        let new_base = if u_bit {
            base.wrapping_add(count.wrapping_mul(4))
        } else {
            base.wrapping_sub(count.wrapping_mul(4))
        };

        let is_user_bank = s_bit && (!l_bit || (rlist & (1 << 15)) == 0);

        if is_user_bank {
            self.swap_user_bank();
        }

        let mut addr = lowest_addr;
        let mut first = true;

        for reg in 0..16 {
            if (rlist & (1 << reg)) != 0 {
                if l_bit {
                    let val = bus.read32(addr & !3, self.r[15]);
                    self.r[reg] = val;
                } else {
                    let val = if reg == 15 {
                        self.r[15].wrapping_add(4)
                    } else if reg == rn && first && w_bit {
                        base
                    } else {
                        self.r[reg]
                    };
                    bus.write32(addr & !3, val);
                }

                addr = addr.wrapping_add(4);
                first = false;
            }
        }

        if is_user_bank {
            self.swap_user_bank();
        }

        // LDM: if base register is in list, loaded value overrides writeback!
        if w_bit && (!l_bit || (rlist & (1 << rn)) == 0) {
            self.r[rn] = new_base;
        }

        if l_bit && (rlist & (1 << 15)) != 0 {
            if s_bit {
                self.restore_spsr();
            }
            self.flush_pipeline(bus);
            3 + count
        } else {
            1 + count
        }
    }

    fn arm_multiply(&mut self, op: u32) -> u32 {
        let accumulate = op & (1 << 21) != 0;
        let set_flags = op & (1 << 20) != 0;
        let rd = ((op >> 16) & 0xF) as usize;
        let rn = ((op >> 12) & 0xF) as usize;
        let rs = ((op >> 8) & 0xF) as usize;
        let rm = (op & 0xF) as usize;

        let mut res = self.r[rm].wrapping_mul(self.r[rs]);
        if accumulate {
            res = res.wrapping_add(self.r[rn]);
        }
        self.r[rd] = res;

        if set_flags {
            let mut flags = self.cpsr & !(FLAG_N | FLAG_Z);
            if res & 0x8000_0000 != 0 { flags |= FLAG_N; }
            if res == 0 { flags |= FLAG_Z; }
            self.cpsr = flags;
        }
        if accumulate { 3 } else { 2 }
    }

    fn arm_multiply_long(&mut self, op: u32) -> u32 {
        let signed = op & (1 << 22) != 0;
        let accumulate = op & (1 << 21) != 0;
        let set_flags = op & (1 << 20) != 0;
        let rdhi = ((op >> 16) & 0xF) as usize;
        let rdlo = ((op >> 12) & 0xF) as usize;
        let rs = ((op >> 8) & 0xF) as usize;
        let rm = (op & 0xF) as usize;

        let result: u64 = if signed {
            ((self.r[rm] as i32 as i64).wrapping_mul(self.r[rs] as i32 as i64)) as u64
        } else {
            (self.r[rm] as u64).wrapping_mul(self.r[rs] as u64)
        };

        let result = if accumulate {
            let acc = ((self.r[rdhi] as u64) << 32) | (self.r[rdlo] as u64);
            result.wrapping_add(acc)
        } else {
            result
        };

        self.r[rdhi] = (result >> 32) as u32;
        self.r[rdlo] = result as u32;

        if set_flags {
            let mut flags = self.cpsr & !(FLAG_N | FLAG_Z);
            if (result >> 63) != 0 { flags |= FLAG_N; }
            if result == 0 { flags |= FLAG_Z; }
            self.cpsr = flags;
        }

        if accumulate { 4 } else { 3 }
    }

    fn arm_swap(&mut self, op: u32, bus: &mut Bus) -> u32 {
        let b_bit = op & (1 << 22) != 0;
        let rn = ((op >> 16) & 0xF) as usize;
        let rd = ((op >> 12) & 0xF) as usize;
        let rm = (op & 0xF) as usize;

        let addr = self.r[rn];
        let write_val = self.r[rm];

        if b_bit {
            let read_val = bus.read8(addr, self.r[15]) as u32;
            bus.write8(addr, write_val as u8);
            self.r[rd] = read_val;
        } else {
            let read_val = bus.read32(addr & !3, self.r[15]);
            let rotated = read_val.rotate_right((addr & 3) * 8);
            bus.write32(addr & !3, write_val);
            self.r[rd] = rotated;
        }
        4
    }

    fn arm_mrs(&mut self, op: u32) -> u32 {
        let rd = ((op >> 12) & 0xF) as usize;
        let spsr = (op & (1 << 22)) != 0;
        let val = if spsr { self.spsr_val() } else { self.cpsr };
        self.r[rd] = val;
        1
    }

    fn arm_msr(&mut self, op: u32) -> u32 {
        let i_bit = op & (1 << 25) != 0;
        let spsr = (op & (1 << 22)) != 0;
        let mask_bits = (op >> 16) & 0xF;

        let val = if i_bit {
            let imm = op & 0xFF;
            let rot = ((op >> 8) & 0xF) * 2;
            imm.rotate_right(rot)
        } else {
            let rm = (op & 0xF) as usize;
            self.r[rm]
        };

        let mut mask = 0u32;
        if mask_bits & 1 != 0 { mask |= 0x0000_00FF; } // control
        if mask_bits & 2 != 0 { mask |= 0x0000_FF00; } // extension
        if mask_bits & 4 != 0 { mask |= 0x00FF_0000; } // status
        if mask_bits & 8 != 0 { mask |= 0xFF00_0000; } // flags

        // User mode cannot modify control/mode bits of CPSR
        if self.mode == Mode::User && !spsr {
            mask &= 0xFF00_0000;
        }

        if spsr {
            self.set_spsr_val(val, mask);
        } else {
            let new_cpsr = (self.cpsr & !mask) | (val & mask);
            if mask & MODE_MASK != 0 {
                let new_mode = Mode::from_bits(new_cpsr);
                self.swap_mode(new_mode);
            }
            self.cpsr = (self.cpsr & !mask) | (val & mask);
        }
        1
    }
}
