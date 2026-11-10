//! Reset must return the emulator to exactly the state of a freshly
//! created instance. Reported in review (2026-11-09): a value latched by
//! an earlier DMA survived `reset()`, so a DMA from an unreadable source
//! returned it instead of 0.

use gba_emu::gba::Gba;

fn with_rom(rom_words: &[u32]) -> Box<Gba> {
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

/// Immediate 32-bit DMA3 of one word from `src` to `dst`; returns the word
/// that arrived.
fn dma3_word(g: &mut Gba, src: u32, dst: u32) -> u32 {
    g.bus.write32(dst, 0x1234_5678);
    g.bus.write32(0x0400_00D4, src);
    g.bus.write32(0x0400_00D8, dst);
    // count = 1, control: enable (bit 15) | 32-bit (bit 10), immediate
    g.bus.write32(0x0400_00DC, 0x8400_0001);
    g.bus.read32(dst, 0)
}

const IDLE_ROM: &[u32] = &[0xEAFF_FFFE]; // b .
const UNREADABLE: u32 = 0x0000_0100; // BIOS: DMA cannot read it
const EWRAM: u32 = 0x0200_0000;
const IWRAM_DST: u32 = 0x0300_1000;

#[test]
fn dma_from_unreadable_source_is_zero_on_fresh_instance() {
    let mut g = with_rom(IDLE_ROM);
    assert_eq!(dma3_word(&mut g, UNREADABLE, IWRAM_DST), 0);
}

#[test]
fn dma_latch_does_not_survive_reset() {
    let mut g = with_rom(IDLE_ROM);
    g.bus.write32(EWRAM, 0xDEAD_BEEF);
    assert_eq!(dma3_word(&mut g, EWRAM, IWRAM_DST), 0xDEAD_BEEF);
    // Without a reset the latch is (correctly) visible.
    assert_eq!(dma3_word(&mut g, UNREADABLE, IWRAM_DST + 4), 0xDEAD_BEEF);

    g.reset();
    assert_eq!(
        dma3_word(&mut g, UNREADABLE, IWRAM_DST),
        0,
        "value latched before reset leaked through"
    );
}

/// General check: a used-then-reset instance must behave exactly like a
/// fresh one. Runs a ROM that loops through DMA, timers, IRQ setup and
/// HALT, then compares frames, audio and CPU state over several frames.
#[test]
fn reset_instance_matches_fresh_instance() {
    let rom: &[u32] = &[
        0xE3A0_0301, // mov r0, #0x04000000
        0xE3A0_1902, // mov r1, #0x8000
        0xE280_2C01, // add r2, r0, #0x100
        0xE1C2_10B2, // strh r1, [r2, #2]      TM0CNT_H = enable
        0xE3A0_1008, // mov r1, #8
        0xE5C0_1004, // strb r1, [r0, #4]      DISPSTAT vblank irq
        0xE3A0_1403, // mov r1, #0x03000000
        0xE580_10D4, // str r1, [r0, #0xD4]    DMA3SAD = IWRAM
        0xE3A0_1406, // mov r1, #0x06000000
        0xE580_10D8, // str r1, [r0, #0xD8]    DMA3DAD = VRAM
        0xE3A0_1102, // mov r1, #0x80000000
        0xE381_1040, // orr r1, r1, #0x40      count 64
        0xE580_10DC, // str r1, [r0, #0xDC]    DMA3CNT (16-bit, immediate)
        0xE3A0_1A01, // mov r1, #0x1000
        0xE281_1001, // add r1, r1, #1
        0xE580_1080, // str r1, [r0, #0x80]    SOUNDCNT_L
        0xEAFF_FFF2, // b to start (loop)
    ];
    let snapshot = |g: &mut Gba| {
        let mut out = Vec::new();
        for _ in 0..4 {
            g.run_frame();
            let fb: u64 = g
                .bus
                .ppu
                .framebuffer
                .iter()
                .fold(0xcbf29ce484222325, |h, &p| (h ^ p as u64).wrapping_mul(0x100000001b3));
            let audio: Vec<i16> = g.audio_buffer[..g.audio_sample_count].to_vec();
            out.push((fb, audio, g.cpu.r, g.cpu.cpsr, g.bus.now));
        }
        out
    };

    let mut fresh = with_rom(rom);
    let expected = snapshot(&mut fresh);

    let mut used = with_rom(rom);
    used.bus.write32(EWRAM, 0xDEAD_BEEF);
    dma3_word(&mut used, EWRAM, IWRAM_DST);
    for _ in 0..7 {
        used.run_frame();
    }
    used.reset();
    assert!(snapshot(&mut used) == expected, "reset instance diverged from fresh instance");
}
