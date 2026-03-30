/// A single measurement point sampled during a discharge run.
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct DischargePoint {
    /// Elapsed time in seconds since the discharge started.
    pub time_s: u16,
    /// Terminal voltage in millivolts at this instant.
    pub voltage_mv: u16,
    /// Load current in milliamps at this instant.
    pub current_ma: u16,
}

const EMPTY_POINT: DischargePoint = DischargePoint {
    time_s: 0,
    voltage_mv: 0,
    current_ma: 0,
};

/// Maximum number of data points that can be stored per discharge run.
///
/// At one sample per second this covers 30 minutes of discharge time.
/// Each point occupies 6 bytes, so the array uses ≈ 10.5 KiB of static RAM.
pub const MAX_DISCHARGE_POINTS: usize = 1_800;

/// Accumulated data for a single discharge run.
///
/// Declared as `static mut` in `main.rs` to keep its ≈10.5 KiB array in BSS,
/// not on the stack.
pub struct DischargeCurve {
    points: [DischargePoint; MAX_DISCHARGE_POINTS],
    count: usize,
    /// Voltage at the first recorded sample (millivolts).
    pub start_voltage_mv: u16,
    /// Total extracted capacity in milli-amp-hours (set when run ends).
    pub total_capacity_mah: u32,
    /// `true` once the run ends (cutoff reached or manually stopped).
    pub is_complete: bool,
}

impl DischargeCurve {
    /// Create a zeroed curve – suitable for use in a `static` initialiser.
    pub const fn new() -> Self {
        Self {
            points: [EMPTY_POINT; MAX_DISCHARGE_POINTS],
            count: 0,
            start_voltage_mv: 0,
            total_capacity_mah: 0,
            is_complete: false,
        }
    }

    /// Reset for a fresh discharge run without changing `cutoff_voltage_mv`.
    pub fn reset(&mut self) {
        self.count = 0;
        self.start_voltage_mv = 0;
        self.total_capacity_mah = 0;
        self.is_complete = false;
    }

    /// Append one measurement to the curve.
    ///
    /// Silently stops recording once `MAX_DISCHARGE_POINTS` is reached.
    pub fn record(&mut self, time_s: u16, voltage_mv: u32, current_ma: u32) {
        if self.count == 0 {
            self.start_voltage_mv = voltage_mv as u16;
        }
        if self.count < MAX_DISCHARGE_POINTS {
            self.points[self.count] = DischargePoint {
                time_s,
                voltage_mv: voltage_mv as u16,
                current_ma: current_ma as u16,
            };
            self.count += 1;
        }
    }

    /// Mark the run as finished and store the final capacity.
    pub fn finalize(&mut self, capacity_mah: u32) {
        self.total_capacity_mah = capacity_mah;
        self.is_complete = true;
    }

    /// Number of points recorded so far.
    #[allow(dead_code)]
    pub fn count(&self) -> usize {
        self.count
    }

    /// Elapsed duration of the recorded discharge (seconds from first to last point).
    #[allow(dead_code)]
    pub fn duration_s(&self) -> u16 {
        if self.count > 0 {
            self.points[self.count - 1].time_s
        } else {
            0
        }
    }

    /// Minimum terminal voltage observed during the discharge (millivolts).
    ///
    /// Returns 0 if no points have been recorded yet.
    #[allow(dead_code)]
    pub fn min_voltage_mv(&self) -> u16 {
        let mut min = u16::MAX;
        for p in &self.points[..self.count] {
            if p.voltage_mv < min {
                min = p.voltage_mv;
            }
        }
        if min == u16::MAX {
            0
        } else {
            min
        }
    }
}
