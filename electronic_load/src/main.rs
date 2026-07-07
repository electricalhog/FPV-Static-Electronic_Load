#![no_std]
#![no_main]

use panic_halt as _;
use rp2040_hal as hal;
use hal::pac;
use embedded_hal::{digital::OutputPin, pwm::SetDutyCycle};
use embedded_hal_0_2::adc::OneShot;
use rotary_encoder_embedded::{Direction, RotaryEncoder};

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

// ── Tuning constants ──────────────────────────────────────────────────────────

/// Control loop sample period. 500 µs = 2 kHz.
const SAMPLE_PERIOD_US: u64 = 500;

/// Current setpoint change per encoder click, milliamps.
const ENCODER_STEP_MA: u32 = 100;

/// Hard ceiling on the setpoint, milliamps.
const MAX_SETPOINT_MA: u32 = 20_000;

/// Soft-start ramp: milliamps added to the active setpoint per control tick.
/// At 2 kHz: 10 mA × 2000 ticks/s = 20 A/s ramp rate.
const SOFT_START_RAMP_MA_PER_TICK: u32 = 10;

/// Overcurrent fault margin above the active setpoint, milliamps.
/// If measured current exceeds (setpoint + this value), the load latches off.
const OVERCURRENT_FAULT_MA: u32 = 2_000;

// ── ADC calibration ───────────────────────────────────────────────────────────
//
// The INA amplifier output is linear: 0 V → 0 mA, Vout_max → MAX_SETPOINT_MA.
// Compute ADC_COUNTS_AT_MAX_MA from your circuit:
//   counts = (Vout_at_max_current / Vref) × 4095
//
// Example — INA194 A1 (gain = 50), Rshunt = 1 mΩ, Imax = 20 A:
//   Vout = 50 × 0.001 × 20 = 1.0 V  →  counts = (1.0 / 3.3) × 4095 ≈ 1241

const ADC_COUNTS_AT_MAX_MA: u32 = 1241;

// ── PI controller gains ───────────────────────────────────────────────────────
//
// Integer arithmetic scaled by GAIN_SCALE avoids floating point while
// preserving sub-unit resolution in the gains.
//
// Tuning procedure:
//   1. Set KI = 0.  Increase KP until current tracks without oscillation.
//   2. Freeze KP.   Slowly raise KI until steady-state error reaches zero.
//      If the output starts hunting, reduce KI.
//
// The anti-windup clamp (inside PiController::update) keeps the integrator
// within the PWM output range so recovery from saturation is immediate.

const GAIN_SCALE: i64 = 1_000;

/// Proportional gain × GAIN_SCALE.  (PWM counts per mA of error)
const KP: i64 = 3;

/// Integral gain × GAIN_SCALE.  (PWM counts per mA per tick)
/// Start at 0 and increase after KP is stable.
const KI: i64 = 0;

// ─────────────────────────────────────────────────────────────────────────────

struct PiController {
    integral: i64,
}

impl PiController {
    const fn new() -> Self {
        Self { integral: 0 }
    }

    /// One PI update step.
    ///
    /// `error_ma`   – signed error: setpoint minus measured current, milliamps.
    /// `max_output` – PWM top value from `channel.max_duty_cycle()`.
    ///
    /// Returns the new PWM duty cycle clamped to `[0, max_output]`.
    fn update(&mut self, error_ma: i32, max_output: u16) -> u16 {
        let ceiling = max_output as i64 * GAIN_SCALE;

        self.integral += KI * error_ma as i64;
        // Anti-windup: the integrator alone cannot exceed output limits, so
        // once the error reverses the output responds in one tick.
        self.integral = self.integral.clamp(0, ceiling);

        let output = (KP * error_ma as i64 + self.integral) / GAIN_SCALE;
        output.clamp(0, max_output as i64) as u16
    }

    fn reset(&mut self) {
        self.integral = 0;
    }
}

// ─────────────────────────────────────────────────────────────────────────────

#[rp2040_hal::entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();

    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);
    let clocks = hal::clocks::init_clocks_and_plls(
        12_000_000,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .unwrap();

    let sio = hal::Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // Rotary encoder: GPIO8 = A, GPIO9 = B
    let pin_a = pins.gpio8.into_pull_up_input();
    let pin_b = pins.gpio9.into_pull_up_input();
    let mut encoder = RotaryEncoder::new(pin_a, pin_b).into_standard_mode();

    // PWM gate drive: GPIO27 (PWM5B), phase-correct for symmetric switching
    let mut pwm_slices = hal::pwm::Slices::new(pac.PWM, &mut pac.RESETS);
    let pwm = &mut pwm_slices.pwm5;
    pwm.set_ph_correct();
    pwm.enable();
    let channel = &mut pwm.channel_b;
    channel.output_to(pins.gpio27);
    let max_duty = channel.max_duty_cycle();

    // DISABLE pin: GPIO6, active-high disables the load
    let mut disable_pin = pins.gpio6.into_push_pull_output();
    let _ = disable_pin.set_high(); // safe default: load off at startup

    // ADC: INA current sense output on GPIO26 (ADC0)
    let mut adc = hal::Adc::new(pac.ADC, &mut pac.RESETS);
    let mut current_pin =
        hal::adc::AdcPin::new(pins.gpio26.into_floating_input()).unwrap();

    // Microsecond timer for rate-gating the control loop
    let timer = hal::Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);

    let mut pi = PiController::new();
    let mut setpoint_ma: u32 = 0;
    let mut active_setpoint_ma: u32 = 0; // soft-started value tracking setpoint_ma
    let mut faulted = false;
    let mut last_tick_us: u64 = timer.get_counter().ticks();

    loop {
        // ── Encoder: update demanded setpoint ────────────────────────────────
        match encoder.update() {
            Direction::Clockwise => {
                setpoint_ma = (setpoint_ma + ENCODER_STEP_MA).min(MAX_SETPOINT_MA);
            }
            Direction::Anticlockwise => {
                setpoint_ma = setpoint_ma.saturating_sub(ENCODER_STEP_MA);
            }
            Direction::None => {}
        }

        // ── Rate gate: run the control loop at a fixed sample period ─────────
        let now_us: u64 = timer.get_counter().ticks();
        if now_us.wrapping_sub(last_tick_us) < SAMPLE_PERIOD_US {
            continue;
        }
        last_tick_us = now_us;

        // ── Fault latch: stay off until power-cycled ──────────────────────────
        if faulted {
            let _ = disable_pin.set_high();
            let _ = channel.set_duty_cycle(0);
            pi.reset();
            continue;
        }

        // ── Soft start: ramp active_setpoint toward setpoint ─────────────────
        if active_setpoint_ma < setpoint_ma {
            active_setpoint_ma =
                (active_setpoint_ma + SOFT_START_RAMP_MA_PER_TICK).min(setpoint_ma);
        } else {
            // Reducing setpoint: step-down immediately (safe direction)
            active_setpoint_ma = setpoint_ma;
        }

        // ── ADC: 4× oversample to reduce quantisation noise ──────────────────
        let measured_ma = {
            let sum: u32 = (0..4u8)
                .map(|_| adc.read(&mut current_pin).unwrap_or(0) as u32)
                .sum();
            adc_to_ma(sum / 4)
        };

        // ── Overcurrent protection ────────────────────────────────────────────
        if measured_ma > active_setpoint_ma.saturating_add(OVERCURRENT_FAULT_MA) {
            faulted = true;
            continue;
        }

        // ── PI control loop ───────────────────────────────────────────────────
        if active_setpoint_ma == 0 {
            let _ = disable_pin.set_high();
            let _ = channel.set_duty_cycle(0);
            pi.reset();
        } else {
            let error_ma = active_setpoint_ma as i32 - measured_ma as i32;
            let pwm_out = pi.update(error_ma, max_duty);
            let _ = disable_pin.set_low();
            let _ = channel.set_duty_cycle(pwm_out);
        }
    }
}

/// Scale a 12-bit ADC reading linearly to milliamps.
///
/// Adjust `ADC_COUNTS_AT_MAX_MA` at the top of this file to calibrate.
#[inline]
fn adc_to_ma(counts: u32) -> u32 {
    counts.saturating_mul(MAX_SETPOINT_MA) / ADC_COUNTS_AT_MAX_MA
}
