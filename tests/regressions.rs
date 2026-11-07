//! Regression tests for hardware behaviours fixed during the timing /
//! audio rework. Each test builds a tiny ROM (hand-encoded ARM/Thumb)
//! or pokes the bus directly, so they run natively without the corpus.

use gba_emu::gba::Gba;

fn boot(rom_words: &[u32]) -> Box<Gba> {
    let mut g = Box::new(Gba::new());
    let mut rom = Vec::new();
    for w in rom_words {
        rom.extend_from_slice(&w.to_le_bytes());
    }
    rom.resize(0x200, 0);
    g.rom_buffer[..rom.len()].copy_from_slice(&rom);
    g.load_rom(rom.len());
    g
}

fn steps(g: &mut Gba, n: usize) {
    for _ in 0..n {
        g.step_one();
    }
}

/// A branch whose refill lands on the same r15 value (here: BX into
/// Thumb at the next word) used to be mistaken for "no branch" and an
/// extra ARM word was fetched, corrupting execution.
#[test]
fn bx_to_following_word_switches_to_thumb() {
    let mut g = boot(&[
        0xE28F_0001, // add r0, pc, #1   -> r0 = 0x08000009 (thumb)
        0xE12F_FF10, // bx r0
        0x2000_212A, // thumb @08: mov r1,#42 ; @0A: mov r0,#0
        0x0000_E7FE, // thumb @0C: b .
    ]);
    steps(&mut g, 20);
    assert_eq!(g.cpu.r[1], 42);
    assert_eq!(g.cpu.cpsr & 0x20, 0x20, "should be in Thumb state");
}

/// `b .` (branch to self) refills to the same r15; must keep looping in
/// place instead of running off.
#[test]
fn branch_to_self_stays_put() {
    let mut g = boot(&[0xEAFF_FFFE]); // b .
    steps(&mut g, 50);
    assert_eq!(g.cpu.r[15], 0x0800_0008);
}

/// Writing HALTCNT must halt the CPU until an enabled IRQ is flagged.
#[test]
fn haltcnt_halts_until_irq() {
    let mut g = boot(&[
        0xE3A0_0301, // mov r0, #0x04000000
        0xE3A0_1001, // mov r1, #1
        0xE5C0_1004, // strb r1, [r0, #4]      DISPSTAT byte write (status bits must survive)
        0xE3A0_1008, // mov r1, #8
        0xE5C0_1004, // strb r1, [r0, #4]      DISPSTAT.vblank_irq = 1
        0xE2802C02, // add r2, r0, #0x200
        0xE1C2_10B0, // strh r1, [r2]          IE = 8 (overwritten below)
        0xE3A0_1001, // mov r1, #1
        0xE1C2_10B0, // strh r1, [r2]          IE = VBlank
        0xE3A0_1000, // mov r1, #0
        0xE5C2_1101, // strb r1, [r2, #0x101]  HALTCNT (0x04000301)
        0xE3A0_3007, // mov r3, #7             runs only after wake
        0xEAFF_FFFE, // b .
    ]);
    // Before VBlank (first VBlank is ~200k cycles in), the CPU must halt
    // and stay halted. (A DISPSTAT byte write used to clear the VBlank
    // status bit and fire a spurious VBlank IRQ on the next line.)
    steps(&mut g, 40);
    assert!(g.cpu.halted, "CPU should be halted");
    assert_eq!(g.bus.io.if_, 0, "no spurious IRQ");
    assert_ne!(g.cpu.r[3], 7);
    g.run_frame();
    steps(&mut g, 10);
    assert!(!g.cpu.halted);
    assert_eq!(g.cpu.r[3], 7);
}

/// In bitmap modes 0x06018000..0x0601BFFF is unmapped (reads 0, writes
/// dropped); in tile modes it mirrors 0x06010000.
#[test]
fn vram_upper_mirror_depends_on_mode() {
    let mut g = boot(&[0xEAFF_FFFE]);
    g.bus.write32(0x0601_0000, 0xABAD_1DEA);
    g.bus.ppu.dispcnt = 0; // mode 0
    assert_eq!(g.bus.read32(0x0601_8000, 0), 0xABAD_1DEA);
    g.bus.ppu.dispcnt = 3; // mode 3
    g.bus.write32(0x0601_8000, 0xDEAD_BEEF);
    assert_eq!(g.bus.read32(0x0601_8000, 0), 0);
    assert_eq!(g.bus.read32(0x0601_0000, 0), 0xABAD_1DEA);
}

/// Out-of-range cartridge reads return the address bus (addr/2).
#[test]
fn rom_out_of_bounds_returns_address_pattern() {
    let mut g = boot(&[0xEAFF_FFFE]);
    assert_eq!(g.bus.read16(0x0900_0000, 0), ((0x0100_0000u32 >> 1) & 0xFFFF) as u16);
}

/// BIOS reads from outside the BIOS return the last fetched BIOS
/// opcode, not the BIOS contents.
#[test]
fn bios_is_read_protected_outside_bios() {
    let mut g = boot(&[0xEAFF_FFFE]);
    steps(&mut g, 4);
    let raw = u32::from_le_bytes([g.bus.bios[0], g.bus.bios[1], g.bus.bios[2], g.bus.bios[3]]);
    assert_ne!(raw, 0);
    assert_ne!(g.bus.read32(0, 0), raw);
}

/// Square channel 2 at frequency register n must oscillate at
/// 131072 / (2048 - n) Hz (it was 4x too fast before the APU rewrite).
#[test]
fn psg_square_pitch() {
    let mut g = boot(&[0xEAFF_FFFE]);
    let n: u32 = 1750; // 131072/298 = 439.8 Hz
    for (a, v) in [
        (0x0400_0084u32, 0x80u8), // master on
        (0x0400_0080, 0x77),      // PSG L/R volume 7
        (0x0400_0081, 0x22),      // ch2 to L and R
        (0x0400_0082, 0x02),      // PSG 100%
        (0x0400_0068, 0x80),      // duty 50%
        (0x0400_0069, 0xF0),      // env: vol 15, no sweep
        (0x0400_006C, (n & 0xFF) as u8),
        (0x0400_006D, 0x80 | (n >> 8) as u8), // trigger
    ] {
        g.bus.write8(a, v);
    }
    g.bus.apu.audio_buffer.clear();
    g.bus.apu.step_cycles(16_777_216 / 4); // 0.25 s
    let left: Vec<i16> = g.bus.apu.audio_buffer.iter().step_by(2).copied().collect();
    let rising = left.windows(2).filter(|w| w[0] <= 0 && w[1] > 0).count();
    let hz = rising as f64 * 4.0;
    assert!((hz - 439.8).abs() < 8.0, "measured {hz} Hz");
}

/// Timer counts start 2 cycles after the enabling write and stop 1 cycle
/// after the disabling write (NBA hw-test timer/start-stop, as graded).
#[test]
fn timer_start_and_stop_latency() {
    let mut g = boot(&[0xEAFF_FFFE]);
    g.bus.write16(0x0400_0102, 0x0080); // start TM0, prescaler 1
    g.bus.now += g.bus.take_cycles() as u64;
    g.bus.now += 10;
    g.bus.sync_timers();
    assert_eq!(g.bus.timers.t[0].counter, 8, "10 cycles minus 2 start delay");
    g.bus.write16(0x0400_0102, 0x0000); // stop
    g.bus.now += g.bus.take_cycles() as u64;
    g.bus.now += 10;
    g.bus.sync_timers();
    let c = g.bus.timers.t[0].counter;
    assert!(c >= 9 && c <= 10, "counts one more cycle after stop, got {c}");
}

