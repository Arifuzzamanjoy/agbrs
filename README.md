# GBA Emulator (Rust / WebAssembly)

An original Game Boy Advance (GBA) emulator written in Rust from scratch, compiled to WebAssembly (`wasm32-unknown-unknown`).

## Features & Subsystems

- **CPU (ARM7TDMI)**: Full implementation of ARM (32-bit) and Thumb (16-bit) instruction sets, barrel shifter, condition evaluation, and 3-stage execution pipeline.
- **Memory Bus**: Complete memory mapping for BIOS, EWRAM, IWRAM, I/O registers, Palette RAM, VRAM, OAM, Game Pak ROM, and SRAM with 16-bit bus alignment and open-bus reads.
- **PPU (Graphics)**: 240x160 scanline renderer supporting Modes 0–5 (tiled and bitmap graphics), affine background layers, OAM sprite rendering (affine and regular), windowing, and alpha blending.
- **DMA Controller**: 4 DMA channels with configurable priorities, repeat modes, and audio FIFO streaming triggers.
- **Timers & Interrupts**: 4 hardware timers with cascading prescalers and IRQ generation.
- **APU (Audio)**: PSG channels (square waves, noise, wave RAM) and DirectSound FIFO channels A & B sampled at 32,768 Hz stereo.

## Building

Prerequisites:
- [Rust toolchain](https://rustup.rs/) (edition 2021)
- `wasm32-unknown-unknown` target:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```

Build the release WebAssembly binary:
```bash
cargo build --release --lib --target wasm32-unknown-unknown
```

The compiled output will be generated at:
`target/wasm32-unknown-unknown/release/gba_emu.wasm`

## Running Tests

Run native unit tests:
```bash
cargo test
```

## ABI Interface

The WebAssembly core exports the standard C-ABI functions:
- `emu_init() -> i32`
- `emu_rom_buffer() -> *mut u8`
- `emu_load_rom(len: i32) -> i32`
- `emu_reset() -> i32`
- `emu_set_keys(keys: u32)`
- `emu_run_frame()`
- `emu_framebuffer() -> *const u32`
- `emu_audio_buffer() -> *const i16`
- `emu_audio_samples() -> i32`
- `emu_audio_rate() -> i32`
