# spec/

`gba_bios_stub.bin` is copied unchanged from gba-eval
(`spec/gba_bios_stub.bin` in https://github.com/mechanize-work/gba-eval).
It is Mechanize's clean-room BIOS stub, not a Nintendo BIOS dump, and is
licensed under MIT (copyright Mechanize, Inc.; see gba-eval's `LEGAL.md`),
not under this repository's Apache-2.0 license. `src/gba.rs` embeds it
with `include_bytes!`.
