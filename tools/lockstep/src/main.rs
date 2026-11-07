//! Instruction-level lockstep tracer: our core (native rlib) vs the
//! gba-eval Mesen2 reference (`reference/mesen.wasm`, via its debug
//! exports `mesen_exec_one`, `mesen_pc`, `mesen_master_clock`).
//!
//! Usage:
//!   gba-lockstep <mesen.wasm> <rom> [--steps N] [--frames-probe N] [--show N]
//!
//! Modes:
//! * default: step both one instruction at a time from the first ROM
//!   instruction, report the first PC divergence, cumulative cycle drift,
//!   and a table of the PCs whose per-instruction cycle cost differs.
//! * --frames-probe N: run N frames on Mesen via emu_run_frame and print
//!   the master clock at each frame boundary (frame phase discovery).
//!
//! Note: this is a development tool; nothing here ships in the wasm.

use anyhow::{anyhow, Result};
use gba_emu::gba::Gba;
use std::collections::HashMap;
use wasmtime::{Engine, Instance, Linker, Module, Store, Val};

struct Mesen {
    store: Store<()>,
    inst: Instance,
}

impl Mesen {
    fn new(path: &str) -> Result<Self> {
        let engine = Engine::default();
        let module = Module::from_file(&engine, path)?;
        let mut store = Store::new(&engine, ());
        let mut linker = Linker::new(&engine);
        // Stub every import (WASI/emscripten) to return zeros.
        for imp in module.imports() {
            if let wasmtime::ExternType::Func(fty) = imp.ty() {
                let results: Vec<_> = fty.results().collect();
                linker.func_new(imp.module(), imp.name(), fty.clone(), move |_c, _p, out| {
                    for (o, t) in out.iter_mut().zip(results.iter()) {
                        *o = match t {
                            wasmtime::ValType::I64 => Val::I64(0),
                            wasmtime::ValType::F32 => Val::F32(0),
                            wasmtime::ValType::F64 => Val::F64(0),
                            _ => Val::I32(0),
                        };
                    }
                    Ok(())
                })?;
            }
        }
        let inst = linker.instantiate(&mut store, &module)?;
        if let Ok(f) = inst.get_typed_func::<(), ()>(&mut store, "_initialize") {
            f.call(&mut store, ())?;
        }
        Ok(Self { store, inst })
    }
    fn call0<R: wasmtime::WasmResults>(&mut self, name: &str) -> Result<R> {
        let f = self.inst.get_typed_func::<(), R>(&mut self.store, name)?;
        Ok(f.call(&mut self.store, ())?)
    }
    fn load(&mut self, rom: &[u8]) -> Result<()> {
        let ok: i32 = self.call0("emu_init")?;
        if ok == 0 {
            return Err(anyhow!("emu_init failed"));
        }
        let ptr: i32 = self.call0("emu_rom_buffer")?;
        let mem = self.inst.get_memory(&mut self.store, "memory").unwrap();
        mem.write(&mut self.store, ptr as usize, rom)?;
        let f = self.inst.get_typed_func::<i32, i32>(&mut self.store, "emu_load_rom")?;
        if f.call(&mut self.store, rom.len() as i32)? == 0 {
            return Err(anyhow!("emu_load_rom failed"));
        }
        Ok(())
    }
    fn pc(&mut self) -> u32 { self.call0::<i32>("mesen_pc").unwrap() as u32 }
    fn clock(&mut self) -> u64 { self.call0::<i64>("mesen_master_clock").unwrap() as u64 }
    fn exec_one(&mut self) -> u64 { self.call0::<i64>("mesen_exec_one").unwrap() as u64 }
}

/// Mesen's `mesen_pc` reports r15 (the pipeline PC), so compare that.
fn our_pc(g: &Gba) -> u32 {
    g.cpu.r[15]
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let get = |f: &str| args.iter().position(|a| a == f).map(|i| args[i + 1].clone());
    let rom = std::fs::read(&args[1])?;
    let mut m = Mesen::new(&args[0])?;
    m.load(&rom)?;

    if let Some(n) = get("--frames-probe") {
        let n: u32 = n.parse()?;
        let mut g = Box::new(Gba::new());
        g.rom_buffer[..rom.len()].copy_from_slice(&rom);
        g.load_rom(rom.len());
        println!("ours after load: clock {} r15 {:08X}", g.bus.now, g.cpu.r[15]);
        for f in 0..n {
            g.run_frame();
            let n = g.drain_audio_samples();
            let c = g.bus.now;
            print!("audio {n:4} ");
            println!("ours  {f:3} clock {c:10} phase {:6} r15 {:08X}", c % 280896, g.cpu.r[15]);
        }
        println!("after load: clock {} pc {:08X}", m.clock(), m.pc());
        for f in 0..n {
            m.call0::<()>("emu_run_frame")?;
            let c = m.clock();
            let n: i32 = m.call0("emu_audio_samples")?;
            println!("frame {f:3} clock {c:10} phase {:6} (line {:3}) pc {:08X} audio pairs {n}", c % 280896, (c % 280896) / 1232, m.pc());
        }
        return Ok(());
    }

    let steps: u64 = get("--steps").map(|s| s.parse().unwrap()).unwrap_or(200_000);
    let show: usize = get("--show").map(|s| s.parse().unwrap()).unwrap_or(25);

    let mut g = Box::new(Gba::new());
    g.rom_buffer[..rom.len()].copy_from_slice(&rom);
    g.load_rom(rom.len());
    println!("start: ours r15 {:08X} clock {} | mesen pc {:08X} clock {}", g.cpu.r[15], g.bus.now, m.pc(), m.clock());

    let mut drift: i64 = 0;
    let mut verbose: u32 = get("--verbose").map(|s| s.parse().unwrap()).unwrap_or(0);
    let mut by_pc: HashMap<(u32, i64), u64> = HashMap::new();
    let mut ring: std::collections::VecDeque<String> = Default::default();
    let mut last_mp = u32::MAX;
    let irq_report = args.iter().any(|a| a == "--irq-report");
    let branch_stats = args.iter().any(|a| a == "--branch-stats");
    let vram_stats = args.iter().any(|a| a == "--vram-stats");
    let mut vstats: HashMap<(u32, u32, bool, u32, u16, i64), u64> = HashMap::new();
    let mut bstats: HashMap<(char, u32, u32, i64), u64> = HashMap::new();
    for i in 0..steps {
        let (mp, mut op) = (m.pc(), our_pc(&g));
        if mp != op && mp == 0x20 {
            // Mesen folds IRQ entry into the previous exec_one; ours is a
            // separate step. Run it and charge it to the previous step.
            g.step_one();
            drift = g.bus.now as i64 - m.clock() as i64;
            op = our_pc(&g);
        }
        let mut mp = mp;
        if mp != op && mp == last_mp {
            // Mesen is stalled (stopped / waking) for a few execs.
            for _ in 0..16 {
                m.exec_one();
                if m.pc() != last_mp { break; }
            }
            mp = m.pc();
        }
        if mp != op && op == 0x20 {
            // We entered the IRQ straight from HALT; Mesen gets there in
            // a few small execs.
            for _ in 0..16 {
                if m.pc() == 0x20 { break; }
                let c = m.exec_one();
                if verbose > 0 { println!("    halt-wake: mesen exec -> pc {:08X} cyc {c} clk {}", m.pc(), m.clock()); }
            }
            if verbose > 0 { println!("    halt-wake: ours r15 {:08X} lr_irq {:08X} clk {}", g.cpu.r[15], g.cpu.r[14], g.bus.now); }
            mp = m.pc();
            let nd = g.bus.now as i64 - m.clock() as i64;
            if nd != drift { *by_pc.entry((0x20, nd - drift)).or_default() += 1; }
            drift = nd;
        }
        if mp != op {
            println!("PC DIVERGENCE at step {i}: mesen {mp:08X} ours {op:08X} (drift {drift} cycles)");
            println!("context (step, r15, mesen cyc, our cyc, our cpsr, ime ie if, vcount.dot, halted):");
            for l in &ring { println!("  {l}"); }
            // Run Mesen ahead to see when it takes the IRQ (line.dot,
            // assuming line 0 starts at clock 3696 as calibrated).
            for k in 0..64 {
                let before = m.clock();
                if m.pc() == 0x20 {
                    let t = before - 3696;
                    println!("mesen reaches IRQ vector after {k} more execs at clock {before} = line {}.{}", (t / 1232) % 228, t % 1232);
                    break;
                }
                m.exec_one();
            }
            break;
        }
        let ctx = format!("{:>3}", "");
        let _ = ctx;
        last_mp = mp;
        let mclk_before = m.clock() as i64;
        let mc = m.exec_one() as i64;
        let mesen_irq_now = m.pc() == 0x20 && mp != 0x20;
        let (cpsr, ime, ie, iff, vc, dot, h) = (g.cpu.cpsr, g.bus.io.ime, g.bus.io.ie, g.bus.io.if_, g.bus.ppu.vcount, g.bus.ppu.scanline_cycles, g.cpu.halted);
        let (vc_before, dot_before) = (g.bus.ppu.vcount, g.bus.ppu.scanline_cycles);
        g.bus.access_log = Some(Vec::new());
        let oc = g.step_one() as i64;
        let log = g.bus.access_log.take().unwrap_or_default();
        if irq_report && mesen_irq_now {
            // Mesen entered the IRQ during this exec (entry = last 3
            // cycles from BIOS). Boundary where it decided:
            let b_take = m.clock() as i64 - 3;
            let b_prev = mclk_before;
            if let Some(&rise) = g.bus.irq_rise_log.iter().rev().find(|&&t| (t as i64) <= b_take) {
                let r = rise as i64;
                println!("IRQ step {i}: rise {r} prev_boundary {:+} take_boundary {:+} instr_len {} pc {mp:08X} thumb {} dot {}",
                    b_prev - r, b_take - r, b_take - b_prev, (g.cpu.cpsr >> 5) & 1, (r - 3696).rem_euclid(1232));
                println!("    ours: since {:?} ime {} ie {:04X} if {:04X} cpsr {:08X} now {}", g.bus.irq_since, g.bus.io.ime, g.bus.io.ie, g.bus.io.if_, g.cpu.cpsr, g.bus.now);
            }
        }
        if vram_stats {
            if let Some(&(a, w, _, _)) = log.iter().find(|e| e.2 == 'D' && (5..=7).contains(&(e.0 >> 24))) {
                if !log.iter().any(|e| e.2 == 'X' || e.2 == 'x' || e.2 == 'M') {
                    let d = (g.bus.now as i64 - m.clock() as i64) - drift;
                    let vis = vc_before < 160;
                    let dot_bucket = if std::env::var("PHASE").is_ok() { (dot_before + log.iter().take_while(|e| e.2 != 'D').map(|e| e.3).sum::<u32>()) % 16 } else { (dot_before / 128) * 128 };
                    *vstats.entry((a >> 24, w, vis, dot_bucket, g.bus.ppu.dispcnt & 7, d)).or_default() += 1;
                }
            }
        }
        if branch_stats {
            if let Some(&(p, c, k, _)) = log.iter().find(|e| e.2 == 'X' || e.2 == 'x') {
                let d = (g.bus.now as i64 - m.clock() as i64) - drift;
                *bstats.entry((k, p, c, d)).or_default() += 1;
            }
        }
        if (g.bus.now as i64 - m.clock() as i64) != drift && verbose > 0 && !g.cpu.halted {
            verbose -= 1;
            println!("delta at step {i} r15 {mp:08X}: mesen {mc} ours {oc} waitcnt {:04X} accesses {:?}", g.bus.io.waitcnt, log);
            for l in ring.iter().rev().take(5).collect::<Vec<_>>().into_iter().rev() { println!("      {l}"); }
            if (oc - mc).abs() > 100 {
                let base = mp.wrapping_sub(0x40) & !3;
                let words: Vec<String> = (0..20).map(|k| format!("{:08X}:{:08X}", base + k * 4, g.bus.read32(base + k * 4, 0))).collect();
                println!("      code: {}", words.join(" "));
                println!("      regs: {:08X?}", &g.cpu.r[..13]);
                for (k, c) in g.bus.dma.channels.iter().enumerate() {
                    println!("      dma{k}: sad {:08X} dad {:08X} count {:04X} ctl {:04X} cur_count {} en {}", c.sad, c.dad, c.count, c.read_control(), c.cur_count, c.enabled);
                }
            }
        }
        ring.push_back(format!("{i:8} {mp:08X} m{mc:4} o{oc:4} cpsr {cpsr:08X} {} {ie:04X} {iff:04X} {vc:3}.{dot:<4} {h}  mclk {}", ime as u8, m.clock()));
        if ring.len() > 14 { ring.pop_front(); }
        // While we skip ahead in HALT, let Mesen catch up in its own
        // small increments (same PC, CPU stopped).
        if g.cpu.halted {
            while (m.clock() as i64) < g.bus.now as i64 && m.pc() == mp {
                m.exec_one();
            }
        }
        let new_drift = g.bus.now as i64 - m.clock() as i64;
        let (mc, oc) = (mc + (m.clock() as i64 - mclk_before - mc), oc);
        if new_drift != drift {
            *by_pc.entry((mp, new_drift - drift)).or_default() += 1;
        }
        drift = new_drift;
        if i % 50_000 == 0 {
            println!("step {i:8} pc {mp:08X} drift {drift}");
        }
    }
    println!("final drift (ours - mesen): {drift} cycles");
    if vram_stats {
        let mut v: Vec<_> = vstats.into_iter().collect();
        v.sort();
        for ((r, w, vis, dot, mode, d), n) in v { println!("vram region {r:X} width {w} visible_line {vis} dot>={dot} mode {mode} delta {d:+} n={n}"); }
    }
    if branch_stats {
        let mut v: Vec<_> = bstats.into_iter().collect();
        v.sort();
        for ((k, p, c, d), n) in v { println!("branch pf_active={} progress={p} count={c} delta={d:+} n={n}", k == 'X'); }
    }
    let mut v: Vec<_> = by_pc.into_iter().collect();
    v.sort_by(|a, b| (b.1 as i64 * b.0 .1.abs()).cmp(&(a.1 as i64 * a.0 .1.abs())));
    println!("{:>10} {:>6} {:>8}  (pc, ours-mesen per exec, count)", "pc", "delta", "count");
    for ((pc, d), n) in v.into_iter().take(show) {
        println!("{pc:10X} {d:6} {n:8}");
    }
    Ok(())
}
