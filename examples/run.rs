//! Native debug runner (not part of the wasm artifact).
//!
//!   cargo run --release --example run -- <rom> <frames> [--replay f] [--dump dir] [--every n] [--trace-frame n]
//!
//! Prints per-frame PC/mode/IE/IF summary, optionally dumps PPM frames
//! (comparable with `oracle run --dump-frames`) and an instruction trace.
use gba_emu::gba::Gba;
use std::io::Write;

fn parse_replay(path: &str) -> Vec<(u32, u32)> {
    let text = std::fs::read_to_string(path).expect("replay");
    let mut ev = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap().trim();
        let mut it = line.split_whitespace();
        if let (Some(f), Some(k)) = (it.next(), it.next()) {
            ev.push((f.parse().unwrap(), u32::from_str_radix(k.trim_start_matches("0x"), 16).unwrap()));
        }
    }
    ev
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rom = std::fs::read(&args[0]).expect("rom");
    let frames: u32 = args[1].parse().unwrap();
    let get = |flag: &str| args.iter().position(|a| a == flag).map(|i| args[i + 1].clone());
    let replay = get("--replay").map(|p| parse_replay(&p)).unwrap_or_default();
    let dump = get("--dump");
    let every: u32 = get("--every").map(|s| s.parse().unwrap()).unwrap_or(1);
    let peek: Vec<u32> = args.iter().enumerate().filter(|(_, a)| *a == "--peek").map(|(i, _)| u32::from_str_radix(args[i + 1].trim_start_matches("0x"), 16).unwrap()).collect();
    let trace_frame: Option<u32> = get("--trace-frame").map(|s| s.parse().unwrap());

    let mut gba = Box::new(Gba::new());
    gba.rom_buffer[..rom.len()].copy_from_slice(&rom);
    gba.load_rom(rom.len());
    if let Some(d) = &dump { std::fs::create_dir_all(d).unwrap(); }
    gba.bus.watch = get("--watch").map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap());
    if let Some(n) = get("--steps").map(|s| s.parse::<u32>().unwrap()) {
        let skip: u32 = get("--skip").map(|s| s.parse().unwrap()).unwrap_or(0);
        for i in 0..skip + n {
            let c = &gba.cpu;
            if i >= skip {
                println!("{i:6} r15 {:08X} T {} pipe {:08X} {:08X} r0 {:08X} r1 {:08X} r2 {:08X} r3 {:08X} sp {:08X} lr {:08X} cpsr {:08X}",
                    c.r[15], (c.cpsr >> 5) & 1, c.pipe[0], c.pipe[1], c.r[0], c.r[1], c.r[2], c.r[3], c.r[13], c.r[14], c.cpsr);
            }
            let cy = gba.cpu.step(&mut gba.bus);
            let _ = cy;
        }
        return;
    }
    let wav = get("--wav");
    let mut audio: Vec<i16> = Vec::new();
    // Match the grader: burn the boot frames the core reports.
    for _ in 0..gba_emu::emu_boot_frames() {
        gba.run_frame();
        gba.drain_audio_samples();
    }
    let mut keys = 0;
    for f in 0..frames {
        for &(ef, k) in &replay { if ef == f { keys = k; } }
        gba.set_keys(keys);
        if trace_frame == Some(f) {
            gba.trace = Some(Vec::new());
        }
        gba.run_frame();
        if let Some(t) = gba.trace.take() {
            let mut out = std::io::BufWriter::new(std::fs::File::create("trace.txt").unwrap());
            for (pc, op, thumb) in t { writeln!(out, "{pc:08X} {} {op:08X}", if thumb { "T" } else { "A" }).unwrap(); }
            eprintln!("wrote trace.txt");
        }
        let n = gba.drain_audio_samples() as usize;
        audio.extend_from_slice(&gba.audio_buffer[..n * 2]);
        if f % every == 0 {
            let c = &gba.cpu;
            println!("frame {f:5} pc {:08X} cpsr {:08X} halted {} ime {} ie {:04X} if {:04X} dispcnt {:04X} vcount {}",
                c.r[15], c.cpsr, c.halted, gba.bus.io.ime, gba.bus.io.ie, gba.bus.io.if_, gba.bus.ppu.dispcnt, gba.bus.ppu.vcount);
            if args.iter().any(|a| a == "--ppu") {
                let p = &gba.bus.ppu;
                println!("    dispcnt {:04X} bgcnt {:04X?} hofs {:?} vofs {:?} win0h {:04X} win0v {:04X} win1h {:04X} win1v {:04X} winin {:04X} winout {:04X} bldcnt {:04X} bldalpha {:04X} bldy {:04X} mosaic {:04X}",
                    p.dispcnt, p.bgcnt, p.bghofs, p.bgvofs, p.win0h, p.win0v, p.win1h, p.win1v, p.winin, p.winout, p.bldcnt, p.bldalpha, p.bldy, p.mosaic);
                println!("    bg2 pa {} pb {} pc {} pd {} x {:08X} y {:08X} cur {:08X} {:08X}", p.bg2pa, p.bg2pb, p.bg2pc, p.bg2pd, p.bg2x, p.bg2y, p.bg2_cur_x, p.bg2_cur_y);
            }
            for &a in &peek {
                let v = gba.bus.read32(a, 0);
                println!("    [{a:08X}] = {v:08X}");
            }
            if let Some(d) = &dump {
                let mut out = std::io::BufWriter::new(std::fs::File::create(format!("{d}/frame_{f:05}.ppm")).unwrap());
                write!(out, "P6\n240 160\n255\n").unwrap();
                for px in gba.bus.ppu.framebuffer.iter() {
                    out.write_all(&[(*px & 0xFF) as u8, (*px >> 8 & 0xFF) as u8, (*px >> 16 & 0xFF) as u8]).unwrap();
                }
            }
        }
    }
    if let Some(w) = wav {
        let mut out = std::io::BufWriter::new(std::fs::File::create(w).unwrap());
        let data_len = (audio.len() * 2) as u32;
        out.write_all(b"RIFF").unwrap();
        out.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        out.write_all(b"WAVEfmt ").unwrap();
        out.write_all(&16u32.to_le_bytes()).unwrap();
        out.write_all(&1u16.to_le_bytes()).unwrap();
        out.write_all(&2u16.to_le_bytes()).unwrap();
        out.write_all(&32768u32.to_le_bytes()).unwrap();
        out.write_all(&(32768u32 * 4).to_le_bytes()).unwrap();
        out.write_all(&4u16.to_le_bytes()).unwrap();
        out.write_all(&16u16.to_le_bytes()).unwrap();
        out.write_all(b"data").unwrap();
        out.write_all(&data_len.to_le_bytes()).unwrap();
        for s in audio { out.write_all(&s.to_le_bytes()).unwrap(); }
    }
}
