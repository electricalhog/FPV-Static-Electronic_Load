/// Operating mode for the electronic load.
///
/// All setpoint values are stored in milli-units:
///   CC  → milliamps  (mA)
///   CP  → milliwatts (mW)
///   CR  → milliohms  (mΩ)
///   CV  → millivolts (mV)
///   Discharge → milliamps (mA), same as CC but also records a discharge curve
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlMode {
    /// Constant Current – maintain a fixed current draw.
    ConstantCurrent,
    /// Constant Power – maintain a fixed power dissipation.
    ConstantPower,
    /// Constant Resistance – emulate a fixed resistive load.
    ConstantResistance,
    /// Constant Voltage – regulate the terminal voltage via a software P‑loop.
    ConstantVoltage,
    /// Discharge test – run CC until the terminal voltage falls to the cutoff,
    /// recording a full voltage/current/time discharge curve.
    Discharge,
}

impl ControlMode {
    /// Cycle to the next mode.
    pub fn next(self) -> Self {
        match self {
            Self::ConstantCurrent => Self::ConstantPower,
            Self::ConstantPower => Self::ConstantResistance,
            Self::ConstantResistance => Self::ConstantVoltage,
            Self::ConstantVoltage => Self::Discharge,
            Self::Discharge => Self::ConstantCurrent,
        }
    }

    /// Two-to-three-letter abbreviation for the display header.
    pub fn short_name(self) -> &'static str {
        match self {
            Self::ConstantCurrent => "CC",
            Self::ConstantPower => "CP",
            Self::ConstantResistance => "CR",
            Self::ConstantVoltage => "CV",
            Self::Discharge => "DIS",
        }
    }

    /// Unit of the setpoint (displayed after the numeric value).
    pub fn unit(self) -> &'static str {
        match self {
            Self::ConstantCurrent => "A",
            Self::ConstantPower => "W",
            Self::ConstantResistance => "Ohm",
            Self::ConstantVoltage => "V",
            Self::Discharge => "A",
        }
    }

    /// Encoder step size when the load is **off** (coarse adjustment, milli-units).
    pub fn idle_step(self) -> u32 {
        match self {
            Self::ConstantCurrent => 500,    // 500 mA
            Self::ConstantPower => 1_000,    // 1 W
            Self::ConstantResistance => 500, // 500 mΩ
            Self::ConstantVoltage => 100,    // 100 mV
            Self::Discharge => 500,          // 500 mA
        }
    }

    /// Encoder step size when the load is **on** (fine adjustment, milli-units).
    pub fn active_step(self) -> u32 {
        match self {
            Self::ConstantCurrent => 100,   // 100 mA
            Self::ConstantPower => 500,     // 500 mW
            Self::ConstantResistance => 100, // 100 mΩ
            Self::ConstantVoltage => 50,    // 50 mV
            Self::Discharge => 100,         // 100 mA
        }
    }

    /// Maximum allowed setpoint (milli-units).
    pub fn max_setpoint(self) -> u32 {
        match self {
            Self::ConstantCurrent => MAX_CURRENT_MA,
            Self::ConstantPower => MAX_POWER_MW,
            Self::ConstantResistance => 100_000, // 100 Ω
            Self::ConstantVoltage => MAX_VOLTAGE_MV,
            Self::Discharge => MAX_CURRENT_MA,
        }
    }

    /// Default setpoint when switching into this mode (milli-units).
    pub fn default_setpoint(self) -> u32 {
        match self {
            Self::ConstantCurrent => 1_000,    // 1 A
            Self::ConstantPower => 5_000,      // 5 W
            Self::ConstantResistance => 1_000, // 1 Ω
            Self::ConstantVoltage => 11_100,   // 11.1 V (3S LiPo nominal)
            Self::Discharge => 1_000,          // 1 A
        }
    }
}

// ─── Physical limits ──────────────────────────────────────────────────────────

/// Maximum load current in milliamps (30 A full-scale).
pub const MAX_CURRENT_MA: u32 = 30_000;
/// Maximum power dissipation in milliwatts (300 W).
pub const MAX_POWER_MW: u32 = 300_000;
/// Maximum supported input voltage in millivolts (26 V, safe for 6S LiPo).
pub const MAX_VOLTAGE_MV: u32 = 26_000;

// ─── Setpoint computation ────────────────────────────────────────────────────

/// Compute the desired load **current** in milliamps from the selected mode,
/// the user's setpoint, the measured terminal voltage, and the current
/// measured current (needed for the CV proportional loop).
///
/// All values use milli-units (mA, mW, mΩ, mV).
pub fn compute_current_ma(
    mode: ControlMode,
    setpoint: u32,
    voltage_mv: u32,
    current_ma: u32,
) -> u32 {
    match mode {
        // CC and Discharge: setpoint *is* the desired current.
        ControlMode::ConstantCurrent | ControlMode::Discharge => setpoint.min(MAX_CURRENT_MA),

        // CP: I = P / V  (mW / mV = A → × 1000 for mA)
        ControlMode::ConstantPower => {
            if voltage_mv > 0 {
                (setpoint * 1_000 / voltage_mv).min(MAX_CURRENT_MA)
            } else {
                0
            }
        }

        // CR: I = V / R  (mV / mΩ = A → × 1000 for mA)
        ControlMode::ConstantResistance => {
            if setpoint > 0 {
                (voltage_mv * 1_000 / setpoint).min(MAX_CURRENT_MA)
            } else {
                // R → 0 means short circuit; clamp to maximum current.
                MAX_CURRENT_MA
            }
        }

        // CV: proportional controller – adjust current to regulate voltage.
        // Gain: 1 mA per mV of error (can be tuned per hardware).
        ControlMode::ConstantVoltage => {
            const KP: u32 = 1;
            if voltage_mv > setpoint {
                let err = voltage_mv - setpoint;
                current_ma.saturating_add(err * KP).min(MAX_CURRENT_MA)
            } else if setpoint > voltage_mv {
                let err = setpoint - voltage_mv;
                current_ma.saturating_sub(err * KP)
            } else {
                current_ma
            }
        }
    }
}

// ─── DAC conversion ──────────────────────────────────────────────────────────

/// Convert a current demand in milliamps to a PWM duty-cycle value.
///
/// The PWM output is filtered externally to produce an analogue DAC signal
/// that drives the op-amp set-point of the current-regulation loop.
///
/// `max_duty` is the value returned by `channel.max_duty_cycle()`.
///
/// # Range assumptions
/// `current_ma` ≤ `MAX_CURRENT_MA` = 30 000 and `max_duty` ≤ 65 535 (u16),
/// so the product fits in a u64 without overflow.
pub fn current_to_dac(current_ma: u32, max_duty: u16) -> u16 {
    // Linear mapping: MAX_CURRENT_MA → max_duty, 0 → 0.
    let duty =
        (current_ma as u64 * max_duty as u64 / MAX_CURRENT_MA as u64) as u32;
    duty.min(max_duty as u32) as u16
}
