//! Game Boy Advance Picture Processing Unit (PPU).

pub const SCREEN_WIDTH: usize = 240;
pub const SCREEN_HEIGHT: usize = 160;
pub const TOTAL_SCANLINES: usize = 228;
pub const CYCLES_PER_SCANLINE: u32 = 1232;
/// Dot at which the HBlank flag, HBlank IRQ and HBlank DMA fire. The
/// visible draw ends at 960, but the flag is raised ~46 cycles later
/// (GBATEK; confirmed by lockstep against the reference).
pub const HDRAW_CYCLES: u32 = 1006;
/// Extra cycles before the HBlank IRQ line rises (lockstep vs reference,
/// together with IRQ_LATENCY_CYCLES: the line rises in dots 1012..1017).
pub const HBLANK_IRQ_LAG: u32 = 8;

#[derive(Clone, Copy)]
pub struct Pixel {
    pub color: u16,       // 15-bit BGR555
    pub priority: u8,    // 0..3 (0 highest), 4 = backdrop
    pub is_obj: bool,
    pub is_semi_transparent: bool,
    pub layer_id: u8,    // 0..3 for BG0..3, 4 for OBJ, 5 for Backdrop
}

impl Default for Pixel {
    fn default() -> Self {
        Self {
            color: 0,
            priority: 4,
            is_obj: false,
            is_semi_transparent: false,
            layer_id: 5,
        }
    }
}

/// Scanline the PPU is on at power-on (see `reset`).
pub const RESET_VCOUNT: u16 = 225;

pub struct Ppu {
    /// Cycles between the most recent PPU IRQ raise and the end of the
    /// last `step_cycles` call (to timestamp the IRQ line exactly).
    pub irq_overshoot: i32,
    pub vcount: u16,
    pub scanline_cycles: u32,

    pub dispcnt: u16,
    pub dispstat: u16,

    pub bgcnt: [u16; 4],
    pub bghofs: [u16; 4],
    pub bgvofs: [u16; 4],

    // Affine parameters
    pub bg2pa: i16,
    pub bg2pb: i16,
    pub bg2pc: i16,
    pub bg2pd: i16,
    pub bg2x: i32,
    pub bg2y: i32,
    pub bg2_cur_x: i32,
    pub bg2_cur_y: i32,

    pub bg3pa: i16,
    pub bg3pb: i16,
    pub bg3pc: i16,
    pub bg3pd: i16,
    pub bg3x: i32,
    pub bg3y: i32,
    pub bg3_cur_x: i32,
    pub bg3_cur_y: i32,

    // Windows
    pub win0h: u16,
    pub win1h: u16,
    pub win0v: u16,
    pub win1v: u16,
    pub winin: u16,
    pub winout: u16,

    // Blending & Effects
    pub mosaic: u16,
    pub bldcnt: u16,
    pub bldalpha: u16,
    pub bldy: u16,

    // Linear ARGB8888 framebuffer (240x160)
    pub framebuffer: Box<[u32; SCREEN_WIDTH * SCREEN_HEIGHT]>,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            vcount: 0,
            scanline_cycles: 0,
            irq_overshoot: 0,
            dispcnt: 0x0080, // Forced blank initially
            dispstat: 0,
            bgcnt: [0; 4],
            bghofs: [0; 4],
            bgvofs: [0; 4],
            bg2pa: 0x0100,
            bg2pb: 0,
            bg2pc: 0,
            bg2pd: 0x0100,
            bg2x: 0,
            bg2y: 0,
            bg2_cur_x: 0,
            bg2_cur_y: 0,
            bg3pa: 0x0100,
            bg3pb: 0,
            bg3pc: 0,
            bg3pd: 0x0100,
            bg3x: 0,
            bg3y: 0,
            bg3_cur_x: 0,
            bg3_cur_y: 0,
            win0h: 0,
            win1h: 0,
            win0v: 0,
            win1v: 0,
            winin: 0,
            winout: 0,
            mosaic: 0,
            bldcnt: 0,
            bldalpha: 0,
            bldy: 0,
            framebuffer: Box::new([0xFF000000; SCREEN_WIDTH * SCREEN_HEIGHT]),
        }
    }

    pub fn reset(&mut self) {
        // Power-on phase matched to the reference (Mesen2 with boot skip):
        // its first VBlank begins at clock ~200816, i.e. line 0 starts
        // 3 lines after reset.
        self.vcount = RESET_VCOUNT;
        self.scanline_cycles = 0;
        self.dispcnt = 0x0080; // BIOS exits with forced blank set
        self.dispstat = 1; // in VBlank
        self.bgcnt = [0; 4];
        self.bghofs = [0; 4];
        self.bgvofs = [0; 4];
        self.bg2pa = 0x0100;
        self.bg2pb = 0;
        self.bg2pc = 0;
        self.bg2pd = 0x0100;
        self.bg2x = 0;
        self.bg2y = 0;
        self.bg2_cur_x = 0;
        self.bg2_cur_y = 0;
        self.bg3pa = 0x0100;
        self.bg3pb = 0;
        self.bg3pc = 0;
        self.bg3pd = 0x0100;
        self.bg3x = 0;
        self.bg3y = 0;
        self.bg3_cur_x = 0;
        self.bg3_cur_y = 0;
        self.win0h = 0;
        self.win1h = 0;
        self.win0v = 0;
        self.win1v = 0;
        self.winin = 0;
        self.winout = 0;
        self.mosaic = 0;
        self.bldcnt = 0;
        self.bldalpha = 0;
        self.bldy = 0;
        self.framebuffer.fill(0xFF000000);
    }

    #[inline(always)]
    pub fn bgr555_to_argb(color: u16) -> u32 {
        let r5 = (color & 0x1F) as u32;
        let g5 = ((color >> 5) & 0x1F) as u32;
        let b5 = ((color >> 10) & 0x1F) as u32;

        let r8 = (r5 << 3) | (r5 >> 2);
        let g8 = (g5 << 3) | (g5 >> 2);
        let b8 = (b5 << 3) | (b5 >> 2);

        0xFF00_0000 | (b8 << 16) | (g8 << 8) | r8
    }

    pub fn render_scanline(&mut self, palette: &[u8], vram: &[u8], oam: &[u8]) {
        let y = self.vcount as usize;
        if y >= SCREEN_HEIGHT {
            return;
        }

        // Forced blank: render pure white
        if self.dispcnt & (1 << 7) != 0 {
            for x in 0..SCREEN_WIDTH {
                self.framebuffer[y * SCREEN_WIDTH + x] = 0xFFFF_FFFF;
            }
            return;
        }

        let bg_mode = self.dispcnt & 7;
        let backdrop_color = u16::from_le_bytes([palette[0], palette[1]]) & 0x7FFF;

        let mut bg_line = [Pixel {
            color: backdrop_color,
            priority: 4,
            is_obj: false,
            is_semi_transparent: false,
            layer_id: 5,
        }; SCREEN_WIDTH];

        let mut obj_line = [Pixel {
            color: 0,
            priority: 4,
            is_obj: true,
            is_semi_transparent: false,
            layer_id: 4,
        }; SCREEN_WIDTH];

        let mut obj_window = [false; SCREEN_WIDTH];

        // 1. Render Sprites (OBJ) if enabled
        if self.dispcnt & (1 << 12) != 0 {
            self.render_scanline_obj(y, palette, vram, oam, &mut obj_line, &mut obj_window);
        }

        // 2. Render Backgrounds according to BG Mode
        match bg_mode {
            0 => {
                // Text BG 0..3
                for prio in (0..=3).rev() {
                    for bg_idx in (0..4).rev() {
                        if self.dispcnt & (1 << (8 + bg_idx)) != 0 && (self.bgcnt[bg_idx] & 3) as u8 == prio {
                            self.render_scanline_text_bg(bg_idx, y, palette, vram, &mut bg_line);
                        }
                    }
                }
            }
            1 => {
                // Text BG 0, 1; Affine BG 2
                for prio in (0..=3).rev() {
                    if self.dispcnt & (1 << 10) != 0 && (self.bgcnt[2] & 3) as u8 == prio {
                        self.render_scanline_affine_bg(2, y, palette, vram, &mut bg_line);
                    }
                    for bg_idx in (0..=1).rev() {
                        if self.dispcnt & (1 << (8 + bg_idx)) != 0 && (self.bgcnt[bg_idx] & 3) as u8 == prio {
                            self.render_scanline_text_bg(bg_idx, y, palette, vram, &mut bg_line);
                        }
                    }
                }
            }
            2 => {
                // Affine BG 2, 3
                for prio in (0..=3).rev() {
                    for bg_idx in [3, 2] {
                        if self.dispcnt & (1 << (8 + bg_idx)) != 0 && (self.bgcnt[bg_idx] & 3) as u8 == prio {
                            self.render_scanline_affine_bg(bg_idx, y, palette, vram, &mut bg_line);
                        }
                    }
                }
            }
            3 => {
                // Bitmap Mode 3: 240x160 16-bit
                if self.dispcnt & (1 << 10) != 0 {
                    let (mw, mh) = self.bg_mosaic(2);
                    let base = (y - y % mh) * SCREEN_WIDTH * 2;
                    let prio = (self.bgcnt[2] & 3) as u8;
                    for x in 0..SCREEN_WIDTH {
                        let xs = x - x % mw;
                        if base + xs * 2 + 1 < vram.len() {
                            let col = u16::from_le_bytes([vram[base + xs * 2], vram[base + xs * 2 + 1]]) & 0x7FFF;
                            bg_line[x] = Pixel { color: col, priority: prio, is_obj: false, is_semi_transparent: false, layer_id: 2 };
                        }
                    }
                }
            }
            4 => {
                // Bitmap Mode 4: 240x160 8-bit palette
                if self.dispcnt & (1 << 10) != 0 {
                    let frame_offset = if self.dispcnt & (1 << 4) != 0 { 0xA000 } else { 0 };
                    let (mw, mh) = self.bg_mosaic(2);
                    let base = frame_offset + (y - y % mh) * SCREEN_WIDTH;
                    let prio = (self.bgcnt[2] & 3) as u8;
                    for x in 0..SCREEN_WIDTH {
                        let xs = x - x % mw;
                        if base + xs < vram.len() {
                            let idx = vram[base + xs] as usize;
                            if idx == 0 { continue; }
                            let col = u16::from_le_bytes([palette[idx * 2], palette[idx * 2 + 1]]) & 0x7FFF;
                            bg_line[x] = Pixel { color: col, priority: prio, is_obj: false, is_semi_transparent: false, layer_id: 2 };
                        }
                    }
                }
            }
            5 => {
                // Bitmap Mode 5: 160x128 16-bit
                if self.dispcnt & (1 << 10) != 0 && y < 128 {
                    let frame_offset = if self.dispcnt & (1 << 4) != 0 { 0xA000 } else { 0 };
                    let (mw, mh) = self.bg_mosaic(2);
                    let base = frame_offset + (y - y % mh) * 160 * 2;
                    let prio = (self.bgcnt[2] & 3) as u8;
                    for x in 0..160 {
                        let xs = x - x % mw;
                        if base + xs * 2 + 1 < vram.len() {
                            let col = u16::from_le_bytes([vram[base + xs * 2], vram[base + xs * 2 + 1]]) & 0x7FFF;
                            bg_line[x] = Pixel { color: col, priority: prio, is_obj: false, is_semi_transparent: false, layer_id: 2 };
                        }
                    }
                }
            }
            _ => {}
        }

        // 3. Composite scanline with Windows and Color Blending
        let win0_active = self.dispcnt & (1 << 13) != 0;
        let win1_active = self.dispcnt & (1 << 14) != 0;
        let winobj_active = self.dispcnt & (1 << 15) != 0;
        let any_win_active = win0_active || win1_active || winobj_active;

        let win0_y1 = (self.win0v >> 8) as usize;
        let win0_y2 = (self.win0v & 0xFF) as usize;
        let in_win0_y = if win0_y1 <= win0_y2 { y >= win0_y1 && y < win0_y2 } else { y >= win0_y1 || y < win0_y2 };

        let win1_y1 = (self.win1v >> 8) as usize;
        let win1_y2 = (self.win1v & 0xFF) as usize;
        let in_win1_y = if win1_y1 <= win1_y2 { y >= win1_y1 && y < win1_y2 } else { y >= win1_y1 || y < win1_y2 };

        for x in 0..SCREEN_WIDTH {
            let mut win_flags = 0x3F; // Default: all layers enabled + color effects enabled

            if any_win_active {
                let win0_x1 = (self.win0h >> 8) as usize;
                let win0_x2 = (self.win0h & 0xFF) as usize;
                let in_win0_x = if win0_x1 <= win0_x2 { x >= win0_x1 && x < win0_x2 } else { x >= win0_x1 || x < win0_x2 };

                let win1_x1 = (self.win1h >> 8) as usize;
                let win1_x2 = (self.win1h & 0xFF) as usize;
                let in_win1_x = if win1_x1 <= win1_x2 { x >= win1_x1 && x < win1_x2 } else { x >= win1_x1 || x < win1_x2 };

                if win0_active && in_win0_y && in_win0_x {
                    win_flags = (self.winin & 0x3F) as u8;
                } else if win1_active && in_win1_y && in_win1_x {
                    win_flags = ((self.winin >> 8) & 0x3F) as u8;
                } else if winobj_active && obj_window[x] {
                    win_flags = ((self.winout >> 8) & 0x3F) as u8;
                } else {
                    win_flags = (self.winout & 0x3F) as u8;
                }
            }

            // Select top pixel (OBJ or BG) based on priority
            let obj_p = &obj_line[x];
            let bg_p = &bg_line[x];

            let obj_enabled = win_flags & (1 << 4) != 0;
            let bg_enabled = if bg_p.layer_id < 4 { win_flags & (1 << bg_p.layer_id) != 0 } else { true };

            let (top, bot) = if obj_p.priority < 4 && obj_enabled && (obj_p.priority <= bg_p.priority || !bg_enabled) {
                (obj_p, bg_p)
            } else if bg_enabled {
                (bg_p, obj_p)
            } else {
                // If neither enabled, backdrop
                let backdrop = Pixel { color: backdrop_color, priority: 4, is_obj: false, is_semi_transparent: false, layer_id: 5 };
                self.framebuffer[y * SCREEN_WIDTH + x] = Self::bgr555_to_argb(backdrop.color);
                continue;
            };

            let enable_effects = (win_flags & (1 << 5)) != 0;
            let final_col = if enable_effects {
                self.apply_color_effects(top, bot)
            } else {
                top.color
            };

            self.framebuffer[y * SCREEN_WIDTH + x] = Self::bgr555_to_argb(final_col);
        }
    }

    /// Mosaic block size (w, h) for a BG, or (1, 1) when disabled.
    fn bg_mosaic(&self, bg: usize) -> (usize, usize) {
        if self.bgcnt[bg] & (1 << 6) != 0 {
            (((self.mosaic & 0xF) + 1) as usize, (((self.mosaic >> 4) & 0xF) + 1) as usize)
        } else {
            (1, 1)
        }
    }

    fn render_scanline_text_bg(&self, bg: usize, y: usize, palette: &[u8], vram: &[u8], line: &mut [Pixel; SCREEN_WIDTH]) {
        let cnt = self.bgcnt[bg];
        let prio = (cnt & 3) as u8;
        let char_base = (((cnt >> 2) & 3) as usize) * 0x4000;
        let is_8bpp = cnt & (1 << 7) != 0;
        let screen_base = (((cnt >> 8) & 0x1F) as usize) * 0x800;
        let screen_size = (cnt >> 14) & 3;

        let hofs = (self.bghofs[bg] & 0x1FF) as usize;
        let vofs = (self.bgvofs[bg] & 0x1FF) as usize;
        let (mw, mh) = self.bg_mosaic(bg);
        let y = y - y % mh;
        let py = (y + vofs) & (if screen_size >= 2 { 511 } else { 255 });

        let tile_y = py / 8;
        let in_tile_y = py % 8;

        for x in 0..SCREEN_WIDTH {
            if prio > line[x].priority {
                continue;
            }

            let px = (x - x % mw + hofs) & (if screen_size == 1 || screen_size == 3 { 511 } else { 255 });
            let tile_x = px / 8;
            let in_tile_x = px % 8;

            // Determine screen block for sizes 0..3
            let screen_block = match screen_size {
                0 => 0, // 32x32
                1 => tile_x / 32, // 64x32
                2 => tile_y / 32, // 32x64
                _ => (tile_x / 32) + (tile_y / 32) * 2, // 64x64
            };

            let map_addr = screen_base + screen_block * 0x800 + ((tile_y % 32) * 32 + (tile_x % 32)) * 2;
            if map_addr + 1 >= vram.len() { continue; }
            let map_entry = u16::from_le_bytes([vram[map_addr], vram[map_addr + 1]]);

            let tile_num = (map_entry & 0x3FF) as usize;
            let hflip = map_entry & (1 << 10) != 0;
            let vflip = map_entry & (1 << 11) != 0;
            let pal_num = ((map_entry >> 12) & 0xF) as usize;

            let fine_x = if hflip { 7 - in_tile_x } else { in_tile_x };
            let fine_y = if vflip { 7 - in_tile_y } else { in_tile_y };

            if is_8bpp {
                let tile_addr = char_base + tile_num * 64 + fine_y * 8 + fine_x;
                if tile_addr < vram.len() {
                    let col_idx = vram[tile_addr] as usize;
                    if col_idx != 0 && col_idx * 2 + 1 < palette.len() {
                        let color = u16::from_le_bytes([palette[col_idx * 2], palette[col_idx * 2 + 1]]) & 0x7FFF;
                        line[x] = Pixel { color, priority: prio, is_obj: false, is_semi_transparent: false, layer_id: bg as u8 };
                    }
                }
            } else {
                let tile_addr = char_base + tile_num * 32 + fine_y * 4 + fine_x / 2;
                if tile_addr < vram.len() {
                    let byte = vram[tile_addr];
                    let col_idx = if fine_x % 2 == 0 { (byte & 0x0F) as usize } else { ((byte >> 4) & 0x0F) as usize };
                    if col_idx != 0 {
                        let pal_offset = (pal_num * 16 + col_idx) * 2;
                        if pal_offset + 1 < palette.len() {
                            let color = u16::from_le_bytes([palette[pal_offset], palette[pal_offset + 1]]) & 0x7FFF;
                            line[x] = Pixel { color, priority: prio, is_obj: false, is_semi_transparent: false, layer_id: bg as u8 };
                        }
                    }
                }
            }
        }
    }

    fn render_scanline_affine_bg(&self, bg: usize, y: usize, palette: &[u8], vram: &[u8], line: &mut [Pixel; SCREEN_WIDTH]) {
        let cnt = self.bgcnt[bg];
        let prio = (cnt & 3) as u8;
        let char_base = (((cnt >> 2) & 3) as usize) * 0x4000;
        let screen_base = (((cnt >> 8) & 0x1F) as usize) * 0x800;
        let wrap = cnt & (1 << 13) != 0;
        let screen_size = (cnt >> 14) & 3;

        let dim = match screen_size {
            0 => 128,
            1 => 256,
            2 => 512,
            _ => 1024,
        };

        let (pa, pb, pc, pd, ref_x, ref_y) = if bg == 2 {
            (self.bg2pa as i32, self.bg2pb as i32, self.bg2pc as i32, self.bg2pd as i32, self.bg2_cur_x, self.bg2_cur_y)
        } else {
            (self.bg3pa as i32, self.bg3pb as i32, self.bg3pc as i32, self.bg3pd as i32, self.bg3_cur_x, self.bg3_cur_y)
        };
        // Vertical mosaic: sample with the reference point of the first
        // line of the mosaic block.
        let (mw, mh) = self.bg_mosaic(bg);
        let back = (y % mh) as i32;
        let ref_x = ref_x - back * pb;
        let ref_y = ref_y - back * pd;

        for x in 0..SCREEN_WIDTH {
            let xm = (x - x % mw) as i32;
            let px = (ref_x + xm * pa) >> 8;
            let py = (ref_y + xm * pc) >> 8;

            if prio > line[x].priority {
                continue;
            }

            let (tx, ty) = if wrap {
                (px.rem_euclid(dim), py.rem_euclid(dim))
            } else {
                if px < 0 || px >= dim || py < 0 || py >= dim {
                    continue;
                }
                (px, py)
            };

            let tile_x = (tx as usize) / 8;
            let tile_y = (ty as usize) / 8;
            let in_x = (tx as usize) % 8;
            let in_y = (ty as usize) % 8;

            let tiles_per_row = (dim / 8) as usize;
            let map_addr = screen_base + tile_y * tiles_per_row + tile_x;
            if map_addr >= vram.len() { continue; }
            let tile_num = vram[map_addr] as usize;

            let tile_addr = char_base + tile_num * 64 + in_y * 8 + in_x;
            if tile_addr < vram.len() {
                let col_idx = vram[tile_addr] as usize;
                if col_idx != 0 && col_idx * 2 + 1 < palette.len() {
                    let color = u16::from_le_bytes([palette[col_idx * 2], palette[col_idx * 2 + 1]]) & 0x7FFF;
                    line[x] = Pixel { color, priority: prio, is_obj: false, is_semi_transparent: false, layer_id: bg as u8 };
                }
            }
        }
    }

    fn render_scanline_obj(&self, y: usize, palette: &[u8], vram: &[u8], oam: &[u8], obj_line: &mut [Pixel; SCREEN_WIDTH], obj_window: &mut [bool; SCREEN_WIDTH]) {
        let is_1d_map = self.dispcnt & (1 << 6) != 0;
        let obj_vram_start = 0x10000;

        for spr in (0..128).rev() {
            let base = spr * 8;
            let attr0 = u16::from_le_bytes([oam[base], oam[base + 1]]);
            let attr1 = u16::from_le_bytes([oam[base + 2], oam[base + 3]]);
            let attr2 = u16::from_le_bytes([oam[base + 4], oam[base + 5]]);

            let is_affine = attr0 & (1 << 8) != 0;
            let is_double_or_disabled = attr0 & (1 << 9) != 0;

            if !is_affine && is_double_or_disabled {
                // Disabled sprite
                continue;
            }

            let obj_mode = (attr0 >> 10) & 3;
            if obj_mode == 3 { continue; } // Prohibited

            let is_8bpp = attr0 & (1 << 13) != 0;
            let shape = (attr0 >> 14) & 3;
            let size = (attr1 >> 14) & 3;

            let (width, height) = match (shape, size) {
                (0, 0) => (8, 8),
                (0, 1) => (16, 16),
                (0, 2) => (32, 32),
                (0, 3) => (64, 64),
                (1, 0) => (16, 8),
                (1, 1) => (32, 8),
                (1, 2) => (32, 16),
                (1, 3) => (64, 32),
                (2, 0) => (8, 16),
                (2, 1) => (8, 32),
                (2, 2) => (16, 32),
                (2, 3) => (32, 64),
                _ => (8, 8),
            };

            let bound_w = if is_affine && is_double_or_disabled { width * 2 } else { width };
            let bound_h = if is_affine && is_double_or_disabled { height * 2 } else { height };

            let spr_y = (attr0 & 0xFF) as usize;
            let spr_x = (attr1 & 0x1FF) as usize;

            let (_sy, in_spr_y) = if y >= spr_y && y < spr_y + bound_h {
                (spr_y, y - spr_y)
            } else if spr_y >= 160 && y < spr_y.wrapping_sub(256) + bound_h {
                (spr_y.wrapping_sub(256), y.wrapping_sub(spr_y.wrapping_sub(256)))
            } else {
                continue;
            };

            let prio = ((attr2 >> 10) & 3) as u8;
            let tile_base = (attr2 & 0x3FF) as usize;
            let pal_num = ((attr2 >> 12) & 0xF) as usize;
            // In bitmap modes the lower half of OBJ VRAM holds the
            // bitmap; tiles 0..511 are not displayed.
            if self.dispcnt & 7 >= 3 && tile_base < 512 {
                continue;
            }
            let mosaic = attr0 & (1 << 12) != 0;
            let (omw, omh) = if mosaic {
                ((((self.mosaic >> 8) & 0xF) + 1) as usize, (((self.mosaic >> 12) & 0xF) + 1) as usize)
            } else {
                (1, 1)
            };
            let in_spr_y = in_spr_y.saturating_sub(y % omh);

            if !is_affine {
                let hflip = attr1 & (1 << 12) != 0;
                let vflip = attr1 & (1 << 13) != 0;
                let py = if vflip { height - 1 - in_spr_y } else { in_spr_y };

                for in_spr_x in 0..width {
                    let screen_x = (spr_x + in_spr_x) & 0x1FF;
                    if screen_x >= SCREEN_WIDTH { continue; }

                    if prio > obj_line[screen_x].priority { continue; }

                    // Horizontal mosaic samples the first pixel of the
                    // screen-aligned block (clamped to the sprite).
                    let in_spr_x = in_spr_x.saturating_sub(screen_x % omw);
                    let px = if hflip { width - 1 - in_spr_x } else { in_spr_x };
                    let (color, has_pixel) = self.get_obj_pixel(px, py, width, tile_base, is_8bpp, pal_num, is_1d_map, palette, vram, obj_vram_start);

                    if has_pixel {
                        if obj_mode == 2 {
                            obj_window[screen_x] = true;
                        } else {
                            obj_line[screen_x] = Pixel {
                                color,
                                priority: prio,
                                is_obj: true,
                                is_semi_transparent: obj_mode == 1,
                                layer_id: 4,
                            };
                        }
                    }
                }
            } else {
                // Affine OBJ
                let group = (((attr1 >> 9) & 0x1F) * 32) as usize;
                let pa = i16::from_le_bytes([oam[group + 6], oam[group + 7]]) as i32;
                let pb = i16::from_le_bytes([oam[group + 14], oam[group + 15]]) as i32;
                let pc = i16::from_le_bytes([oam[group + 22], oam[group + 23]]) as i32;
                let pd = i16::from_le_bytes([oam[group + 30], oam[group + 31]]) as i32;

                let cx = (bound_w / 2) as i32;
                let cy = (bound_h / 2) as i32;
                let ox = (width / 2) as i32;
                let oy = (height / 2) as i32;

                let iy = in_spr_y as i32 - cy;

                for in_spr_x in 0..bound_w {
                    let screen_x = (spr_x + in_spr_x) & 0x1FF;
                    if screen_x >= SCREEN_WIDTH { continue; }

                    if prio > obj_line[screen_x].priority { continue; }

                    let ix = in_spr_x as i32 - cx;
                    let tex_x = ((pa * ix + pb * iy) >> 8) + ox;
                    let tex_y = ((pc * ix + pd * iy) >> 8) + oy;

                    if tex_x >= 0 && tex_x < width as i32 && tex_y >= 0 && tex_y < height as i32 {
                        let (color, has_pixel) = self.get_obj_pixel(tex_x as usize, tex_y as usize, width, tile_base, is_8bpp, pal_num, is_1d_map, palette, vram, obj_vram_start);
                        if has_pixel {
                            if obj_mode == 2 {
                                obj_window[screen_x] = true;
                            } else {
                                obj_line[screen_x] = Pixel {
                                    color,
                                    priority: prio,
                                    is_obj: true,
                                    is_semi_transparent: obj_mode == 1,
                                    layer_id: 4,
                                };
                            }
                        }
                    }
                }
            }
        }
    }

    fn get_obj_pixel(&self, px: usize, py: usize, width: usize, tile_base: usize, is_8bpp: bool, pal_num: usize, is_1d: bool, palette: &[u8], vram: &[u8], vram_start: usize) -> (u16, bool) {
        let tile_x = px / 8;
        let tile_y = py / 8;
        let in_x = px % 8;
        let in_y = py % 8;

        let tile_idx = if is_1d {
            let tiles_per_row = width / 8;
            tile_base + (tile_y * tiles_per_row + tile_x) * (if is_8bpp { 2 } else { 1 })
        } else {
            tile_base + tile_y * 32 + tile_x * (if is_8bpp { 2 } else { 1 })
        };

        if is_8bpp {
            let addr = vram_start + tile_idx * 32 + in_y * 8 + in_x;
            if addr < vram.len() {
                let col_idx = vram[addr] as usize;
                if col_idx != 0 && 0x200 + col_idx * 2 + 1 < palette.len() {
                    let pal_addr = 0x200 + col_idx * 2;
                    let color = u16::from_le_bytes([palette[pal_addr], palette[pal_addr + 1]]) & 0x7FFF;
                    return (color, true);
                }
            }
        } else {
            let addr = vram_start + tile_idx * 32 + in_y * 4 + in_x / 2;
            if addr < vram.len() {
                let byte = vram[addr];
                let col_idx = if in_x % 2 == 0 { (byte & 0x0F) as usize } else { ((byte >> 4) & 0x0F) as usize };
                if col_idx != 0 {
                    let pal_addr = 0x200 + (pal_num * 16 + col_idx) * 2;
                    if pal_addr + 1 < palette.len() {
                        let color = u16::from_le_bytes([palette[pal_addr], palette[pal_addr + 1]]) & 0x7FFF;
                        return (color, true);
                    }
                }
            }
        }

        (0, false)
    }

    fn apply_color_effects(&self, top: &Pixel, bot: &Pixel) -> u16 {
        let effect_mode = (self.bldcnt >> 6) & 3;
        let is_1st_target = (self.bldcnt & (1 << top.layer_id)) != 0 || top.is_semi_transparent;

        if !is_1st_target {
            return top.color;
        }

        if top.is_semi_transparent || effect_mode == 1 {
            // Alpha Blending
            let is_2nd_target = (self.bldcnt & (1 << (8 + bot.layer_id))) != 0;
            if is_2nd_target {
                let eva = ((self.bldalpha & 0x1F) as u32).min(16);
                let evb = (((self.bldalpha >> 8) & 0x1F) as u32).min(16);

                let r1 = (top.color & 0x1F) as u32;
                let g1 = ((top.color >> 5) & 0x1F) as u32;
                let b1 = ((top.color >> 10) & 0x1F) as u32;

                let r2 = (bot.color & 0x1F) as u32;
                let g2 = ((bot.color >> 5) & 0x1F) as u32;
                let b2 = ((bot.color >> 10) & 0x1F) as u32;

                let r = ((r1 * eva + r2 * evb) / 16).min(31);
                let g = ((g1 * eva + g2 * evb) / 16).min(31);
                let b = ((b1 * eva + b2 * evb) / 16).min(31);

                return (r | (g << 5) | (b << 10)) as u16;
            }
        } else if effect_mode == 2 {
            // Brightness Increase
            let evy = ((self.bldy & 0x1F) as u32).min(16);
            let r = (top.color & 0x1F) as u32;
            let g = ((top.color >> 5) & 0x1F) as u32;
            let b = ((top.color >> 10) & 0x1F) as u32;

            let new_r = r + ((31 - r) * evy) / 16;
            let new_g = g + ((31 - g) * evy) / 16;
            let new_b = b + ((31 - b) * evy) / 16;

            return (new_r | (new_g << 5) | (new_b << 10)) as u16;
        } else if effect_mode == 3 {
            // Brightness Decrease
            let evy = ((self.bldy & 0x1F) as u32).min(16);
            let r = (top.color & 0x1F) as u32;
            let g = ((top.color >> 5) & 0x1F) as u32;
            let b = ((top.color >> 10) & 0x1F) as u32;

            // floor(I * (16 - EVY) / 16), matching the reference's rounding
            // (I - floor(I * EVY / 16) is one step too bright).
            let new_r = (r * (16 - evy)) / 16;
            let new_g = (g * (16 - evy)) / 16;
            let new_b = (b * (16 - evy)) / 16;

            return (new_r | (new_g << 5) | (new_b << 10)) as u16;
        }

        top.color
    }

    /// Cycles until the next HBlank or line-end event.
    pub fn cycles_to_next_event(&self) -> u32 {
        let sc = self.scanline_cycles;
        if sc < HDRAW_CYCLES { HDRAW_CYCLES - sc } else { CYCLES_PER_SCANLINE.saturating_sub(sc).max(1) }
    }

    pub fn step_cycles(&mut self, cycles: u32, if_reg: &mut u16, palette: &[u8], vram: &[u8], oam: &[u8]) -> (bool, bool) {
        let mut vblank_trigger = false;
        let mut hblank_trigger = false;

        self.scanline_cycles += cycles;

        // Check for HBlank transition at cycle 960
        if self.scanline_cycles >= HDRAW_CYCLES && (self.dispstat & 2) == 0 {
            self.dispstat |= 2; // Set HBlank flag
            if self.vcount < (SCREEN_HEIGHT as u16) {
                hblank_trigger = true;
                if self.dispstat & (1 << 4) != 0 {
                    *if_reg |= 1 << 1; // HBlank IRQ
                    // The IRQ line rises HBLANK_IRQ_LAG after the flag
                    // (negative overshoot = rise is still in the future).
                    self.irq_overshoot = self.scanline_cycles as i32 - (HDRAW_CYCLES + HBLANK_IRQ_LAG) as i32;
                }
                // Render visible scanline during HDRAW
                self.render_scanline(palette, vram, oam);
            }
        }

        if self.scanline_cycles >= CYCLES_PER_SCANLINE {
            self.scanline_cycles -= CYCLES_PER_SCANLINE;
            self.dispstat &= !2; // Clear HBlank flag

            // Advance affine coordinates at end of scanline
            self.bg2_cur_x += self.bg2pb as i32;
            self.bg2_cur_y += self.bg2pd as i32;
            self.bg3_cur_x += self.bg3pb as i32;
            self.bg3_cur_y += self.bg3pd as i32;

            self.vcount += 1;
            if self.vcount >= (TOTAL_SCANLINES as u16) {
                self.vcount = 0;
                // Reload affine reference coordinates at start of frame
                self.bg2_cur_x = self.bg2x;
                self.bg2_cur_y = self.bg2y;
                self.bg3_cur_x = self.bg3x;
                self.bg3_cur_y = self.bg3y;
            }

            // Update VBlank flag
            if self.vcount >= 160 && self.vcount <= 226 {
                self.dispstat |= 1; // VBlank flag
                if self.vcount == 160 {
                    vblank_trigger = true;
                    if self.dispstat & (1 << 3) != 0 {
                        *if_reg |= 1 << 0; // VBlank IRQ
                        self.irq_overshoot = self.scanline_cycles as i32;
                    }
                }
            } else {
                self.dispstat &= !1; // Clear VBlank flag
            }

            // Update V-Counter match flag
            let vcount_setting = (self.dispstat >> 8) & 0xFF;
            if self.vcount == vcount_setting {
                self.dispstat |= 4; // V-Counter match
                if self.dispstat & (1 << 5) != 0 {
                    *if_reg |= 1 << 2; // V-Counter IRQ
                    self.irq_overshoot = self.scanline_cycles as i32;
                }
            } else {
                self.dispstat &= !4;
            }
        }

        (vblank_trigger, hblank_trigger)
    }
}
