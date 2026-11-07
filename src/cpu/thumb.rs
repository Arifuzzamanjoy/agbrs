//! Thumb (16-bit) instruction decoder and executor for ARMv4T.

use super::registers::*;
use super::alu::*;
use super::Cpu;
use crate::bus::Bus;

impl Cpu {
    pub fn exec_thumb(&mut self, op: u16, bus: &mut Bus) -> u32 {
        let op = op as u32;

        // Format 2: Add/subtract (3-register or register+immediate)
        // 00011 [I:1] [Op:1] [Rn/imm3:3] [Rs:3] [Rd:3]
        if (op >> 11) == 0b00011 {
            let i_bit = (op >> 10) & 1 != 0;
            let op_bit = (op >> 9) & 1 != 0;
            let rn_imm = (op >> 6) & 7;
            let rs = ((op >> 3) & 7) as usize;
            let rd = (op & 7) as usize;

            let op2 = if i_bit { rn_imm } else { self.r[rn_imm as usize] };
            let (res, flags) = if !op_bit {
                add(self.r[rs], op2, 0)
            } else {
                sub(self.r[rs], op2, 1)
            };
            self.r[rd] = res;
            self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | flags;
            return 1;
        }

        // Format 1: Move shifted register
        // 000 [Op:2] [Offset5:5] [Rs:3] [Rd:3]
        if (op >> 13) == 0 {
            let shift_op = (op >> 11) & 3;
            let offset = (op >> 6) & 0x1F;
            let rs = ((op >> 3) & 7) as usize;
            let rd = (op & 7) as usize;
            let rm = self.r[rs];
            let carry_in = (self.cpsr & FLAG_C) != 0;

            let shift_type = match shift_op {
                0 => ShiftType::Lsl,
                1 => ShiftType::Lsr,
                _ => ShiftType::Asr,
            };

            let (res, carry_out) = shift_imm(shift_type, rm, offset, carry_in);
            self.r[rd] = res;
            self.cpsr = logic_flags(res, carry_out, self.cpsr);
            return 1;
        }

        // Format 3: Move/compare/add/subtract immediate
        // 001 [Op:2] [Rd:3] [Imm8:8]
        if (op >> 13) == 0b001 {
            let alu_op = (op >> 11) & 3;
            let rd = ((op >> 8) & 7) as usize;
            let imm = op & 0xFF;

            match alu_op {
                0 => { // MOV
                    self.r[rd] = imm;
                    let mut flags = self.cpsr & !(FLAG_N | FLAG_Z);
                    if imm & 0x8000_0000 != 0 { flags |= FLAG_N; }
                    if imm == 0 { flags |= FLAG_Z; }
                    self.cpsr = flags;
                }
                1 => { // CMP
                    let (_, flags) = sub(self.r[rd], imm, 1);
                    self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | flags;
                }
                2 => { // ADD
                    let (res, flags) = add(self.r[rd], imm, 0);
                    self.r[rd] = res;
                    self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | flags;
                }
                3 => { // SUB
                    let (res, flags) = sub(self.r[rd], imm, 1);
                    self.r[rd] = res;
                    self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | flags;
                }
                _ => {}
            }
            return 1;
        }

        // Format 4: ALU operations
        // 010000 [Op:4] [Rs:3] [Rd:3]
        if (op >> 10) == 0b010000 {
            let alu_op = (op >> 6) & 0xF;
            let rs = ((op >> 3) & 7) as usize;
            let rd = (op & 7) as usize;
            let carry_in = (self.cpsr & FLAG_C) != 0;

            match alu_op {
                0x0 => { // AND
                    let res = self.r[rd] & self.r[rs];
                    self.r[rd] = res;
                    self.cpsr = logic_flags(res, carry_in, self.cpsr);
                }
                0x1 => { // EOR
                    let res = self.r[rd] ^ self.r[rs];
                    self.r[rd] = res;
                    self.cpsr = logic_flags(res, carry_in, self.cpsr);
                }
                0x2 => { // LSL
                    let (res, c) = shift_reg(ShiftType::Lsl, self.r[rd], self.r[rs], carry_in);
                    self.r[rd] = res;
                    self.cpsr = logic_flags(res, c, self.cpsr);
                }
                0x3 => { // LSR
                    let (res, c) = shift_reg(ShiftType::Lsr, self.r[rd], self.r[rs], carry_in);
                    self.r[rd] = res;
                    self.cpsr = logic_flags(res, c, self.cpsr);
                }
                0x4 => { // ASR
                    let (res, c) = shift_reg(ShiftType::Asr, self.r[rd], self.r[rs], carry_in);
                    self.r[rd] = res;
                    self.cpsr = logic_flags(res, c, self.cpsr);
                }
                0x5 => { // ADC
                    let (res, flags) = add(self.r[rd], self.r[rs], carry_in as u32);
                    self.r[rd] = res;
                    self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | flags;
                }
                0x6 => { // SBC
                    let (res, flags) = sub(self.r[rd], self.r[rs], carry_in as u32);
                    self.r[rd] = res;
                    self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | flags;
                }
                0x7 => { // ROR
                    let (res, c) = shift_reg(ShiftType::Ror, self.r[rd], self.r[rs], carry_in);
                    self.r[rd] = res;
                    self.cpsr = logic_flags(res, c, self.cpsr);
                }
                0x8 => { // TST
                    let res = self.r[rd] & self.r[rs];
                    self.cpsr = logic_flags(res, carry_in, self.cpsr);
                }
                0x9 => { // NEG
                    let (res, flags) = sub(0, self.r[rs], 1);
                    self.r[rd] = res;
                    self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | flags;
                }
                0xA => { // CMP
                    let (_, flags) = sub(self.r[rd], self.r[rs], 1);
                    self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | flags;
                }
                0xB => { // CMN
                    let (_, flags) = add(self.r[rd], self.r[rs], 0);
                    self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | flags;
                }
                0xC => { // ORR
                    let res = self.r[rd] | self.r[rs];
                    self.r[rd] = res;
                    self.cpsr = logic_flags(res, carry_in, self.cpsr);
                }
                0xD => { // MUL
                    let res = self.r[rd].wrapping_mul(self.r[rs]);
                    self.r[rd] = res;
                    self.cpsr = logic_flags(res, carry_in, self.cpsr);
                }
                0xE => { // BIC
                    let res = self.r[rd] & !self.r[rs];
                    self.r[rd] = res;
                    self.cpsr = logic_flags(res, carry_in, self.cpsr);
                }
                0xF => { // MVN
                    let res = !self.r[rs];
                    self.r[rd] = res;
                    self.cpsr = logic_flags(res, carry_in, self.cpsr);
                }
                _ => {}
            }
            return 1;
        }

        // Format 5: Hi register operations / branch exchange
        // 010001 [Op:2] [H1:1] [H2:1] [Rs/Hs:3] [Rd/Hd:3]
        if (op >> 10) == 0b010001 {
            let hi_op = (op >> 8) & 3;
            let h1 = (op >> 7) & 1;
            let h2 = (op >> 6) & 1;
            let rs = (((op >> 3) & 7) | (h2 << 3)) as usize;
            let rd = ((op & 7) | (h1 << 3)) as usize;

            match hi_op {
                0 => { // ADD
                    let val = self.r[rd].wrapping_add(self.r[rs]);
                    self.r[rd] = val;
                    if rd == 15 {
                        self.r[15] &= !1;
                        self.flush_pipeline(bus);
                        return 3;
                    }
                    return 1;
                }
                1 => { // CMP
                    let (_, flags) = sub(self.r[rd], self.r[rs], 1);
                    self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | flags;
                    return 1;
                }
                2 => { // MOV
                    self.r[rd] = self.r[rs];
                    if rd == 15 {
                        self.r[15] &= !1;
                        self.flush_pipeline(bus);
                        return 3;
                    }
                    return 1;
                }
                3 => { // BX
                    let target = self.r[rs];
                    if target & 1 != 0 {
                        self.cpsr |= FLAG_T;
                    } else {
                        self.cpsr &= !FLAG_T;
                    }
                    self.r[15] = target & !1;
                    self.flush_pipeline(bus);
                    return 3;
                }
                _ => return 1,
            }
        }

        // Format 6: PC-relative load (LDR Rd, [PC, #imm8*4])
        // 01001 [Rd:3] [Imm8:8]
        if (op >> 11) == 0b01001 {
            let rd = ((op >> 8) & 7) as usize;
            let offset = (op & 0xFF) * 4;
            let addr = (self.r[15] & !2).wrapping_add(offset);
            self.r[rd] = bus.read32(addr, self.r[15]);
            return 3;
        }

        // Format 7: Load/store with register offset
        // 0101 [L:1] [B:1] 0 [Ro:3] [Rb:3] [Rd:3]
        if (op >> 12) == 0b0101 && (op & (1 << 9)) == 0 {
            let l_bit = op & (1 << 11) != 0;
            let b_bit = op & (1 << 10) != 0;
            let ro = ((op >> 6) & 7) as usize;
            let rb = ((op >> 3) & 7) as usize;
            let rd = (op & 7) as usize;
            let addr = self.r[rb].wrapping_add(self.r[ro]);

            if l_bit {
                if b_bit {
                    self.r[rd] = bus.read8(addr, self.r[15]) as u32;
                } else {
                    let val = bus.read32(addr & !3, self.r[15]);
                    self.r[rd] = val.rotate_right((addr & 3) * 8);
                }
                return 3;
            } else {
                if b_bit {
                    bus.write8(addr, self.r[rd] as u8);
                } else {
                    bus.write32(addr & !3, self.r[rd]);
                }
                return 2;
            }
        }

        // Format 8: Load/store sign-extended byte/halfword
        // 0101 [H:1] [S:1] 1 [Ro:3] [Rb:3] [Rd:3]
        if (op >> 12) == 0b0101 && (op & (1 << 9)) != 0 {
            let h_bit = op & (1 << 11) != 0;
            let s_bit = op & (1 << 10) != 0;
            let ro = ((op >> 6) & 7) as usize;
            let rb = ((op >> 3) & 7) as usize;
            let rd = (op & 7) as usize;
            let addr = self.r[rb].wrapping_add(self.r[ro]);

            match (h_bit, s_bit) {
                (false, false) => { // STRH
                    bus.write16(addr & !1, self.r[rd] as u16);
                    return 2;
                }
                (false, true) => { // LDSB
                    self.r[rd] = (bus.read8(addr, self.r[15]) as i8) as i32 as u32;
                    return 3;
                }
                (true, false) => { // LDRH
                    let val = bus.read16(addr & !1, self.r[15]) as u32;
                    self.r[rd] = if addr & 1 != 0 { val.rotate_right(8) } else { val };
                    return 3;
                }
                (true, true) => { // LDSH
                    self.r[rd] = if addr & 1 != 0 {
                        (bus.read8(addr, self.r[15]) as i8) as i32 as u32
                    } else {
                        (bus.read16(addr & !1, self.r[15]) as i16) as i32 as u32
                    };
                    return 3;
                }
            }
        }

        // Format 9: Load/store with immediate offset
        // 011 [B:1] [L:1] [Offset5:5] [Rb:3] [Rd:3]
        if (op >> 13) == 0b011 {
            let b_bit = op & (1 << 12) != 0;
            let l_bit = op & (1 << 11) != 0;
            let offset5 = (op >> 6) & 0x1F;
            let rb = ((op >> 3) & 7) as usize;
            let rd = (op & 7) as usize;

            if b_bit {
                let addr = self.r[rb].wrapping_add(offset5);
                if l_bit {
                    self.r[rd] = bus.read8(addr, self.r[15]) as u32;
                    return 3;
                } else {
                    bus.write8(addr, self.r[rd] as u8);
                    return 2;
                }
            } else {
                let addr = self.r[rb].wrapping_add(offset5 * 4);
                if l_bit {
                    let val = bus.read32(addr & !3, self.r[15]);
                    self.r[rd] = val.rotate_right((addr & 3) * 8);
                    return 3;
                } else {
                    bus.write32(addr & !3, self.r[rd]);
                    return 2;
                }
            }
        }

        // Format 10: Load/store halfword
        // 1000 [L:1] [Offset5:5] [Rb:3] [Rd:3]
        if (op >> 12) == 0b1000 {
            let l_bit = op & (1 << 11) != 0;
            let offset = ((op >> 6) & 0x1F) * 2;
            let rb = ((op >> 3) & 7) as usize;
            let rd = (op & 7) as usize;
            let addr = self.r[rb].wrapping_add(offset);

            if l_bit {
                let val = bus.read16(addr & !1, self.r[15]) as u32;
                self.r[rd] = if addr & 1 != 0 { val.rotate_right(8) } else { val };
                return 3;
            } else {
                bus.write16(addr & !1, self.r[rd] as u16);
                return 2;
            }
        }

        // Format 11: SP-relative load/store
        // 1001 [L:1] [Rd:3] [Word8:8]
        if (op >> 12) == 0b1001 {
            let l_bit = op & (1 << 11) != 0;
            let rd = ((op >> 8) & 7) as usize;
            let offset = (op & 0xFF) * 4;
            let addr = self.r[13].wrapping_add(offset);

            if l_bit {
                let val = bus.read32(addr & !3, self.r[15]);
                self.r[rd] = val.rotate_right((addr & 3) * 8);
                return 3;
            } else {
                bus.write32(addr & !3, self.r[rd]);
                return 2;
            }
        }

        // Format 12: Load address
        // 1010 [SP:1] [Rd:3] [Word8:8]
        if (op >> 12) == 0b1010 {
            let sp_bit = op & (1 << 11) != 0;
            let rd = ((op >> 8) & 7) as usize;
            let offset = (op & 0xFF) * 4;

            if !sp_bit {
                self.r[rd] = (self.r[15] & !2).wrapping_add(offset);
            } else {
                self.r[rd] = self.r[13].wrapping_add(offset);
            }
            return 1;
        }

        // Format 13: Add offset to Stack Pointer
        // 10110000 [S:1] [Word7:7]
        if (op >> 8) == 0b10110000 {
            let s_bit = op & (1 << 7) != 0;
            let offset = (op & 0x7F) * 4;
            if !s_bit {
                self.r[13] = self.r[13].wrapping_add(offset);
            } else {
                self.r[13] = self.r[13].wrapping_sub(offset);
            }
            return 1;
        }

        // Format 14: Push/pop registers
        // 1011 [L:1] 10 [R:1] [Rlist:8]
        if (op >> 12) == 0b1011 && ((op >> 9) & 3) == 0b10 {
            let l_bit = op & (1 << 11) != 0;
            let r_bit = op & (1 << 8) != 0;
            let rlist = op & 0xFF;

            if !l_bit {
                // PUSH
                let mut count = 0u32;
                for i in 0..8 {
                    if (rlist & (1 << i)) != 0 { count += 1; }
                }
                if r_bit { count += 1; }

                let start_addr = self.r[13].wrapping_sub(count * 4);
                let mut addr = start_addr;
                for i in 0..8 {
                    if (rlist & (1 << i)) != 0 {
                        bus.write32(addr, self.r[i]);
                        addr = addr.wrapping_add(4);
                    }
                }
                if r_bit {
                    bus.write32(addr, self.r[14]);
                }
                self.r[13] = start_addr;
                return count + 1;
            } else {
                // POP
                let mut addr = self.r[13];
                let mut count = 0u32;
                for i in 0..8 {
                    if (rlist & (1 << i)) != 0 {
                        self.r[i] = bus.read32(addr, self.r[15]);
                        addr = addr.wrapping_add(4);
                        count += 1;
                    }
                }
                if r_bit {
                    let target = bus.read32(addr, self.r[15]);
                    addr = addr.wrapping_add(4);
                    self.r[15] = target & !1;
                    self.flush_pipeline(bus);
                    count += 1;
                }
                self.r[13] = addr;
                return if r_bit { count + 2 } else { count + 1 };
            }
        }

        // Format 15: Multiple load/store
        // 1100 [L:1] [Rb:3] [Rlist:8]
        if (op >> 12) == 0b1100 {
            let l_bit = op & (1 << 11) != 0;
            let rb = ((op >> 8) & 7) as usize;
            let rlist = op & 0xFF;
            let mut addr = self.r[rb];
            let mut count = 0u32;

            if !l_bit {
                // STMIA
                for i in 0..8 {
                    if (rlist & (1 << i)) != 0 {
                        bus.write32(addr, self.r[i]);
                        addr = addr.wrapping_add(4);
                        count += 1;
                    }
                }
                self.r[rb] = addr;
                return count + 1;
            } else {
                // LDMIA
                for i in 0..8 {
                    if (rlist & (1 << i)) != 0 {
                        self.r[i] = bus.read32(addr, self.r[15]);
                        addr = addr.wrapping_add(4);
                        count += 1;
                    }
                }
                if (rlist & (1 << rb)) == 0 {
                    self.r[rb] = addr;
                }
                return count + 2;
            }
        }

        // Format 17: Software Interrupt
        // 1101 1111 [Comment:8]
        if (op >> 8) == 0b11011111 {
            return self.thumb_swi(bus);
        }

        // Format 16: Conditional Branch
        // 1101 [Cond:4] [Soff8:8]
        if (op >> 12) == 0b1101 {
            let cond = (op >> 8) & 0xF;
            if cond < 0xE && self.check_cond(cond) {
                let soff = ((op & 0xFF) as i8 as i32) * 2;
                self.r[15] = (self.r[15] as i32).wrapping_add(soff) as u32;
                self.flush_pipeline(bus);
                return 3;
            }
            return 1;
        }

        // Format 18: Unconditional branch
        // 11100 [Offset11:11]
        if (op >> 11) == 0b11100 {
            let offset = ((op & 0x7FF) as i32) << 21 >> 20; // sign extend 11 bits * 2
            self.r[15] = (self.r[15] as i32).wrapping_add(offset) as u32;
            self.flush_pipeline(bus);
            return 3;
        }

        // Format 19: Long branch with link (BL)
        // Opcode 1: 11110 [Offset11:11]
        // Opcode 2: 11111 [Offset11:11]
        if (op >> 12) == 0b1111 {
            let h_bit = op & (1 << 11) != 0;
            if !h_bit {
                // First instruction
                let offset = ((op & 0x7FF) as i32) << 21 >> 9; // sign extend 11-bit << 12
                self.r[14] = (self.r[15] as i32).wrapping_add(offset) as u32;
                return 1;
            } else {
                // Second instruction
                let next_pc = self.r[15].wrapping_sub(2) | 1;
                let target = (self.r[14] as i32).wrapping_add(((op & 0x7FF) * 2) as i32) as u32;
                self.r[14] = next_pc;
                self.r[15] = target & !1;
                self.flush_pipeline(bus);
                return 3;
            }
        }

        1 // NOP fallback
    }

    fn thumb_swi(&mut self, bus: &mut Bus) -> u32 {
        self.banked.r14_svc = self.r[15].wrapping_sub(2);
        self.banked.spsr_svc = self.cpsr;
        self.swap_mode(Mode::Supervisor);
        self.cpsr |= FLAG_I;
        self.cpsr &= !FLAG_T;
        self.r[15] = 0x0000_0008;
        self.flush_pipeline(bus);
        3
    }
}
