# AI Engineering Conversation: Architecture & Core Implementation

**Platform**: Google DeepMind Antigravity  
**Lead Agent**: Principal Systems Architect  
**Scope**: Project scaffolding, ARM7TDMI CPU, Memory Bus, PPU, DMA, APU, and WASM C-ABI

---

## 1. Project Initialization & C-ABI Setup

**User**:
> Build a Game Boy Advance emulator from scratch in Rust, compiled to WebAssembly (target: `wasm32-unknown-unknown`), following the Mechanize GBA Eval specifications. Must be pure Rust, zero-panic, deterministic, and export the 10 standard C-ABI functions.

**Assistant**:
I will structure the emulator with modular separation of concerns under `src/`:
- `src/lib.rs`: WASM C-ABI exports wrapping the engine
- `src/gba.rs`: Top-level deterministic state container (`Gba`)
- `src/cpu/`: ARM7TDMI core with ARM (`arm.rs`) and Thumb (`thumb.rs`) decoders, ALU (`alu.rs`), banked registers (`registers.rs`)
- `src/bus/`: MMIO and memory mapping (`mod.rs`, `io.rs`, `waitstates.rs`)
- `src/ppu/`: 240x160 scanline renderer (Modes 0-5, affine BGs, sprites, windows, blending)
- `src/dma/`: 4-channel DMA controller
- `src/timer/`: 4 cascading hardware timers
- `src/apu/`: 4 PSG channels + DirectSound FIFO A/B

Setting up `Cargo.toml`:
```toml
[package]
name = "gba_emu"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
```

---

## 2. CPU Architecture: ARM7TDMI Pipeline & Mode Switching

**Assistant**:
The ARM7TDMI uses a 3-stage fetch-decode-execute pipeline. To ensure exact instruction behavior when instructions read register `R15` (PC):
1. In ARM state: PC reads as `Current Instruction + 8`
2. In Thumb state: PC reads as `Current Instruction + 4`

We maintain a 2-element pipeline buffer `pipe: [u32; 2]` in `Cpu`. When a branch or exception occurs, `flush_pipeline()` refills `pipe[0]` and `pipe[1]` from the target address and advances `R15` by 2 instructions ahead.

For mode switching, banked registers are maintained for `User/System`, `FIQ`, `Supervisor`, `Abort`, `IRQ`, and `Undefined` modes. When switching modes via CPSR modification or exception vectoring, registers R8–R14 and SPSR are swapped into their respective bank.

---

## 3. Memory Bus Architecture & MMIO

**Assistant**:
The GBA memory map covers 28-bit address space mirrored into 32-bit:
- `0x00000000..=0x00003FFF`: BIOS (16 KiB stub loaded from `spec/gba_bios_stub.bin`)
- `0x02000000..=0x0203FFFF`: 256 KiB On-Board WRAM (EWRAM), mirrored across 0x02000000–0x02FFFFFF
- `0x03000000..=0x03007FFF`: 32 KiB On-Chip WRAM (IWRAM), mirrored across 0x03000000–0x03FFFFFF
- `0x04000000..=0x040003FE`: I/O Registers (16-bit bus alignment)
- `0x05000000..=0x050003FF`: 1 KiB Palette RAM (mirrored)
- `0x06000000..=0x06017FFF`: 96 KiB VRAM (mirrored)
- `0x07000000..=0x070003FF`: 1 KiB OAM (mirrored)
- `0x08000000..=0x0DFFFFFF`: Game Pak ROM Waitstate areas (0, 1, 2)
- `0x0E000000..=0x0E00FFFF`: Game Pak SRAM (64 KiB)

All unmapped regions implement open-bus reads rather than returning 0 or faulting.

---

## 4. Video Subsystem (PPU)

**Assistant**:
The GBA display is 240×160 pixels. Scanlines run from VCOUNT 0 to 227 (160 visible scanlines + 68 VBlank scanlines). Each scanline consists of 1232 cycles:
- Cycles 0..960: HDRAW
- Cycles 960..1232: HBLANK

Scanline rendering is executed at cycle 960 to produce the composite line buffer:
- **Modes 0, 1, 2**: Tiled backgrounds with text, affine (rotation/scaling), and matrix transformation support.
- **Modes 3, 4, 5**: Direct color bitmap rendering (Mode 3: 240x160 16bpp, Mode 4: 240x160 8bpp indexed double-buffered, Mode 5: 160x128 16bpp).
- **Sprites / OAM**: 128 sprite entries rendered with 1D/2D mapping, affine matrix transformations, priority resolution, and alpha blending.
- **Windowing**: WIN0, WIN1, and OBJ window mask evaluation.

