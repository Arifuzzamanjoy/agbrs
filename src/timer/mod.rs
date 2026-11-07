//! GBA Hardware Timers (Timer 0..3).

/// Delay between the enabling write and counting (tuned against the
/// NBA timer-reload/start-stop tests as graded vs Mesen2).
pub const TIMER_START_DELAY: u32 = 2;
pub const TIMER_STOP_DELAY: u32 = 1;

pub struct Timer {
    pub reload: u16,
    pub counter: u16,
    pub prescaler_shift: u32, // 0, 6, 8, 10
    pub count_up: bool,
    pub irq_enable: bool,
    pub enabled: bool,
    pub sub_cycles: u32,
    pub overflow: bool,
    /// Cycles after enabling before the first prescaler tick.
    pub start_delay: u32,
    /// A disabling write takes effect after this many cycles.
    pub stop_in: u32,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            reload: 0,
            counter: 0,
            prescaler_shift: 0,
            count_up: false,
            irq_enable: false,
            enabled: false,
            sub_cycles: 0,
            overflow: false,
            start_delay: 0,
            stop_in: 0,
        }
    }

    pub fn write_control(&mut self, val: u16) {
        let old_enabled = self.enabled;
        self.prescaler_shift = match val & 3 {
            0 => 0,  // /1
            1 => 6,  // /64
            2 => 8,  // /256
            _ => 10, // /1024
        };
        self.count_up = (val & (1 << 2)) != 0;
        self.irq_enable = (val & (1 << 6)) != 0;
        self.enabled = (val & (1 << 7)) != 0;
        self.stop_in = 0;
        if old_enabled && !self.enabled && !self.count_up {
            // Keeps counting for one more cycle (NBA start-stop test).
            self.enabled = true;
            self.stop_in = TIMER_STOP_DELAY;
        }

        if !old_enabled && self.enabled {
            self.counter = self.reload;
            self.sub_cycles = 0;
            self.start_delay = TIMER_START_DELAY;
        }
    }

    pub fn read_control(&self) -> u16 {
        let prescaler_bits = match self.prescaler_shift {
            0 => 0,
            6 => 1,
            8 => 2,
            _ => 3,
        };
        prescaler_bits
            | ((self.count_up as u16) << 2)
            | ((self.irq_enable as u16) << 6)
            | (((self.enabled && self.stop_in == 0) as u16) << 7)
    }

    pub fn step_cycles(&mut self, mut cycles: u32) -> bool {
        if !self.enabled || self.count_up {
            return false;
        }
        let mut stopping = false;
        if self.stop_in > 0 {
            cycles = cycles.min(self.stop_in);
            stopping = true;
        }
        if self.start_delay > 0 {
            let d = self.start_delay.min(cycles);
            self.start_delay -= d;
            cycles -= d;
        }
        let r = self.count_cycles(cycles);
        if stopping {
            self.stop_in = 0;
            self.enabled = false;
        }
        r
    }

    fn count_cycles(&mut self, cycles: u32) -> bool {

        self.sub_cycles += cycles;
        let period = 1 << self.prescaler_shift;
        let ticks = self.sub_cycles / period;
        self.sub_cycles %= period;

        if ticks == 0 {
            return false;
        }

        let old_counter = self.counter as u32;
        let new_counter = old_counter + ticks;
        if new_counter > 0xFFFF {
            let _overflows = (new_counter - 0x10000) / ((0x10000 - self.reload as u32).max(1)) + 1;
            let rem = (new_counter - 0x10000) % ((0x10000 - self.reload as u32).max(1));
            self.counter = (self.reload as u32 + rem) as u16;
            self.overflow = true;
            true
        } else {
            self.counter = new_counter as u16;
            false
        }
    }

    pub fn inc_count_up(&mut self) -> bool {
        if !self.enabled || !self.count_up {
            return false;
        }

        if self.counter == 0xFFFF {
            self.counter = self.reload;
            self.overflow = true;
            true
        } else {
            self.counter += 1;
            false
        }
    }
}

pub struct Timers {
    pub t: [Timer; 4],
}

impl Default for Timers {
    fn default() -> Self {
        Self::new()
    }
}

impl Timers {
    /// Cycles until the earliest overflow of a free-running timer.
    pub fn cycles_to_overflow(&self) -> u32 {
        let mut best = u32::MAX;
        for t in &self.t {
            if t.enabled && !t.count_up {
                let period = 1u32 << t.prescaler_shift;
                let ticks = 0x10000 - t.counter as u32;
                let c = t.start_delay + ticks * period - t.sub_cycles;
                best = best.min(c.max(1));
            }
        }
        best
    }

    pub fn new() -> Self {
        Self {
            t: [Timer::new(), Timer::new(), Timer::new(), Timer::new()],
        }
    }

    pub fn step(&mut self, cycles: u32, if_reg: &mut u16) -> (bool, bool) {
        let mut t0_overflow = false;
        let mut t1_overflow = false;

        // Timer 0
        if self.t[0].step_cycles(cycles) {
            t0_overflow = true;
            if self.t[0].irq_enable {
                *if_reg |= 1 << 3;
            }
            // Cascade to Timer 1
            if self.t[1].inc_count_up() {
                t1_overflow = true;
                if self.t[1].irq_enable {
                    *if_reg |= 1 << 4;
                }
                // Cascade to Timer 2
                if self.t[2].inc_count_up() {
                    if self.t[2].irq_enable {
                        *if_reg |= 1 << 5;
                    }
                    // Cascade to Timer 3
                    if self.t[3].inc_count_up() && self.t[3].irq_enable {
                        *if_reg |= 1 << 6;
                    }
                }
            }
        }

        // Timer 1 (if not count-up)
        if !self.t[1].count_up && self.t[1].step_cycles(cycles) {
            t1_overflow = true;
            if self.t[1].irq_enable {
                *if_reg |= 1 << 4;
            }
            if self.t[2].inc_count_up() {
                if self.t[2].irq_enable {
                    *if_reg |= 1 << 5;
                }
                if self.t[3].inc_count_up() && self.t[3].irq_enable {
                    *if_reg |= 1 << 6;
                }
            }
        }

        // Timer 2 (if not count-up)
        if !self.t[2].count_up && self.t[2].step_cycles(cycles) {
            if self.t[2].irq_enable {
                *if_reg |= 1 << 5;
            }
            if self.t[3].inc_count_up() && self.t[3].irq_enable {
                *if_reg |= 1 << 6;
            }
        }

        // Timer 3 (if not count-up)
        if !self.t[3].count_up && self.t[3].step_cycles(cycles) && self.t[3].irq_enable {
            *if_reg |= 1 << 6;
        }

        (t0_overflow, t1_overflow)
    }

    pub fn read8(&self, addr: u32) -> u8 {
        let idx = ((addr - 0x04000100) / 4) as usize;
        let sub = addr & 3;
        if idx >= 4 { return 0; }
        match sub {
            0 => self.t[idx].counter as u8,
            1 => (self.t[idx].counter >> 8) as u8,
            2 => self.t[idx].read_control() as u8,
            3 => (self.t[idx].read_control() >> 8) as u8,
            _ => 0,
        }
    }

    pub fn read16(&self, addr: u32) -> u16 {
        let idx = ((addr - 0x04000100) / 4) as usize;
        let sub = addr & 2;
        if idx >= 4 { return 0; }
        if sub == 0 {
            self.t[idx].counter
        } else {
            self.t[idx].read_control()
        }
    }

    pub fn write8(&mut self, addr: u32, val: u8) {
        let idx = ((addr - 0x04000100) / 4) as usize;
        let sub = addr & 3;
        if idx >= 4 { return; }
        match sub {
            0 => self.t[idx].reload = (self.t[idx].reload & 0xFF00) | (val as u16),
            1 => self.t[idx].reload = (self.t[idx].reload & 0x00FF) | ((val as u16) << 8),
            2 => {
                let cur = self.t[idx].read_control();
                self.t[idx].write_control((cur & 0xFF00) | (val as u16));
            }
            3 => {
                let cur = self.t[idx].read_control();
                self.t[idx].write_control((cur & 0x00FF) | ((val as u16) << 8));
            }
            _ => {}
        }
    }

    pub fn write16(&mut self, addr: u32, val: u16) {
        let idx = ((addr - 0x04000100) / 4) as usize;
        let sub = addr & 2;
        if idx >= 4 { return; }
        if sub == 0 {
            self.t[idx].reload = val;
        } else {
            self.t[idx].write_control(val);
        }
    }
}
