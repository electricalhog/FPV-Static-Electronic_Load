//! FPV Static Electronic Load – firmware for the Raspberry Pi RP2040.
//!
//! # Hardware pin mapping
//!
//! | GPIO | Function                                        |
//! |------|-------------------------------------------------|
//! | 0    | I2C0 SDA → SSD1306 OLED display                |
//! | 1    | I2C0 SCL → SSD1306 OLED display                |
//! | 2    | Encoder push-button (active-LOW, pull-up)       |
//! | 6    | Load DISABLE (HIGH = disabled, LOW = active)    |
//! | 8    | Rotary encoder A                                |
//! | 9    | Rotary encoder B                                |
//! | 26   | ADC0 – terminal voltage (via ÷8 divider)        |
//! | 27   | PWM5-B – filtered DAC → current set-point       |
//! | 28   | ADC2 – load current (via shunt + amplifier)     |
//!
//! # User interface
//!
//! * **Rotate encoder** – adjust the active setpoint value.
//!   Load OFF → coarse steps; Load ON → fine steps.
//! * **Short press** (< 600 ms) – cycle operating mode
//!   CC → CP → CR → CV → DIS → CC (only when load is OFF).
//! * **Long press**  (≥ 600 ms) – toggle load ON / OFF.
//!   Starting in DIS mode also resets the discharge curve.
//!
//! # Operating modes
//!
//! | Mode | Setpoint unit | Description                             |
//! |------|---------------|-----------------------------------------|
//! | CC   | mA            | Constant current                        |
//! | CP   | mW            | Constant power (needs voltage ADC)      |
//! | CR   | mΩ            | Constant resistance (needs voltage ADC) |
//! | CV   | mV            | Constant voltage (P-loop)               |
//! | DIS  | mA            | Discharge curve recording (CC + cutoff) |

#![no_std]
#![no_main]

mod control;
mod discharge;

// ─── Panic / runtime ──────────────────────────────────────────────────────────
use cortex_m_rt as _;
use panic_halt as _;

// ─── HAL ──────────────────────────────────────────────────────────────────────
use rp2040_hal as hal;
use hal::pac;
use rp2040_hal::clocks::Clock;

// ─── Embedded-HAL traits ──────────────────────────────────────────────────────
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal::pwm::SetDutyCycle;
// ADC in rp2040-hal 0.10 still uses the embedded-hal 0.2 OneShot trait.
use embedded_hal_0_2::adc::OneShot;

// ─── Peripherals / drivers ────────────────────────────────────────────────────
use rotary_encoder_embedded::{Direction, RotaryEncoder};
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};
use heapless::String;
use core::fmt::Write as FmtWrite;

// ─── Application modules ──────────────────────────────────────────────────────
use control::{compute_current_ma, current_to_dac, ControlMode};
use discharge::DischargeCurve;

// ─── Boot block ───────────────────────────────────────────────────────────────
#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

// ─── ADC calibration constants ───────────────────────────────────────────────
/// Voltage divider ratio on the voltage-sense input.
/// A ÷8 divider maps 26 V → 3.25 V, within the 3.3 V ADC reference.
const VOLT_DIV_RATIO: u32 = 8;

/// Current-sense scale: millivolts per amp at the ADC input.
/// Example: 10 mΩ shunt + ×10 amplifier → 100 mV/A.
const CURR_MV_PER_A: u32 = 100;

// ─── Default cutoff voltage ──────────────────────────────────────────────────
/// Default under-voltage cutoff for Discharge mode (millivolts).
const DEFAULT_CUTOFF_MV: u32 = 3_300;

// ─── Loop timing ─────────────────────────────────────────────────────────────
/// Main-loop period in microseconds → 1 kHz loop rate.
const LOOP_PERIOD_US: u32 = 1_000;
/// Re-sample the ADC every this many loop iterations (10 ms).
const ADC_PERIOD: u32 = 10;
/// Refresh the display every this many loop iterations (100 ms).
const DISPLAY_PERIOD: u32 = 100;
/// Record one discharge data-point every this many iterations (1 s).
const RECORD_PERIOD: u32 = 1_000;

// ─── Display layout constants ─────────────────────────────────────────────────
/// Y coordinate (pixels from top) of the discharge voltage progress bar.
const VOLTAGE_BAR_Y: i32 = 62;

// ─── Button timing ───────────────────────────────────────────────────────────
/// Stable-state debounce window in loop iterations (= ms at 1 kHz).
const DEBOUNCE_TICKS: u32 = 20;
/// Minimum hold time (ms) to register a long-press.
const LONG_PRESS_MS: u32 = 600;

// ─── Static discharge curve ──────────────────────────────────────────────────
/// Kept as a `static mut` so its ≈10.5 KiB array lives in BSS, not on the stack.
static mut DISCHARGE_CURVE: DischargeCurve = DischargeCurve::new();

// ─── Application state ────────────────────────────────────────────────────────
struct AppState {
    mode: ControlMode,
    /// Active setpoint in milli-units (mA / mW / mΩ / mV).
    setpoint: u32,
    /// Last measured terminal voltage (mV).
    voltage_mv: u32,
    /// Last measured load current (mA).
    current_ma: u32,
    /// Derived power (mW).
    power_mw: u32,
    /// Whether the load MOSFET is enabled.
    load_enabled: bool,
    /// Seconds elapsed since the current run started.
    elapsed_s: u32,
    /// Accumulated charge in milli-amp-hours.
    capacity_mah: u32,
    /// Internal milli-amp-second accumulator for capacity integration.
    capacity_acc_mas: u32,
    /// Under-voltage cutoff for Discharge mode (mV).
    cutoff_mv: u32,
    /// Latched `true` when a discharge run ends at the cutoff.
    discharge_complete: bool,
    /// Demand current from the previous iteration (for CV P-loop, mA).
    demand_ma: u32,
}

impl AppState {
    const fn new() -> Self {
        Self {
            mode: ControlMode::ConstantCurrent,
            setpoint: 1_000,
            voltage_mv: 0,
            current_ma: 0,
            power_mw: 0,
            load_enabled: false,
            elapsed_s: 0,
            capacity_mah: 0,
            capacity_acc_mas: 0,
            cutoff_mv: DEFAULT_CUTOFF_MV,
            discharge_complete: false,
            demand_ma: 0,
        }
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────
#[rp2040_hal::entry]
fn main() -> ! {
    // SAFETY: no other code reads or writes DISCHARGE_CURVE concurrently.
    let discharge_curve: &mut DischargeCurve =
        unsafe { &mut *core::ptr::addr_of_mut!(DISCHARGE_CURVE) };

    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();

    // ── Clocks ────────────────────────────────────────────────────────────────
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

    // ── GPIO ──────────────────────────────────────────────────────────────────
    let sio = hal::Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // ── I2C0 for SSD1306 OLED (GPIO0 = SDA, GPIO1 = SCL, 400 kHz) ─────────
    let i2c = hal::I2C::new_controller(
        pac.I2C0,
        pins.gpio0.reconfigure::<hal::gpio::FunctionI2C, hal::gpio::PullUp>(), // SDA
        pins.gpio1.reconfigure::<hal::gpio::FunctionI2C, hal::gpio::PullUp>(), // SCL
        rp2040_hal::fugit::HertzU32::from_raw(400_000),
        &mut pac.RESETS,
        clocks.system_clock.freq(),
    );

    // SSD1306 in buffered-graphics mode
    let interface = I2CDisplayInterface::new(i2c);
    let mut display =
        Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
            .into_buffered_graphics_mode();
    display.init().unwrap();
    display.clear(BinaryColor::Off).unwrap();
    display.flush().unwrap();

    // ── ADC ───────────────────────────────────────────────────────────────────
    let mut adc = hal::Adc::new(pac.ADC, &mut pac.RESETS);
    let mut voltage_adc_pin =
        hal::adc::AdcPin::new(pins.gpio26.into_floating_input()).unwrap();
    let mut current_adc_pin =
        hal::adc::AdcPin::new(pins.gpio28.into_floating_input()).unwrap();

    // ── Rotary encoder ────────────────────────────────────────────────────────
    let pin_a = pins.gpio8.into_pull_up_input();
    let pin_b = pins.gpio9.into_pull_up_input();
    let mut encoder = RotaryEncoder::new(pin_a, pin_b).into_standard_mode();

    // Encoder push-button (GPIO2, active-LOW with internal pull-up)
    let mut enc_button = pins.gpio2.into_pull_up_input();

    // ── Busy-wait delay ───────────────────────────────────────────────────────
    let mut delay =
        cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());

    // ── PWM DAC output (PWM5-B → GPIO27) ─────────────────────────────────────
    let mut pwm_slices = hal::pwm::Slices::new(pac.PWM, &mut pac.RESETS);
    let pwm = &mut pwm_slices.pwm5;
    pwm.set_ph_correct();
    pwm.enable();
    let channel = &mut pwm.channel_b;
    channel.output_to(pins.gpio27);
    let max_duty = channel.max_duty_cycle();

    // ── Load DISABLE pin (HIGH = off, LOW = on) ────────────────────────────
    let mut disable_pin = pins.gpio6.into_push_pull_output();
    disable_pin.set_high().unwrap(); // start with load off

    // ── Application state ─────────────────────────────────────────────────────
    let mut state = AppState::new();

    // Button debounce / press-type state
    let mut btn_raw_prev = false;
    let mut btn_stable = false;
    let mut btn_stable_ticks: u32 = 0;
    let mut btn_hold_ticks: u32 = 0;
    let mut long_press_fired = false;

    let mut loop_tick: u32 = 0;

    // ═══════════════════════════════════════════════════════════════════════════
    // Main loop – runs at ~1 kHz (LOOP_PERIOD_US = 1 000 µs per iteration)
    // ═══════════════════════════════════════════════════════════════════════════
    loop {
        // ── 1. Encoder rotation ───────────────────────────────────────────────
        let direction = encoder.update();
        let step = if state.load_enabled {
            state.mode.active_step()
        } else {
            state.mode.idle_step()
        };
        match direction {
            Direction::Clockwise => {
                let clamped = state.setpoint.saturating_add(step);
                state.setpoint = clamped.min(state.mode.max_setpoint());
            }
            Direction::Anticlockwise => {
                state.setpoint = state.setpoint.saturating_sub(step);
            }
            Direction::None => {}
        }

        // ── 2. Button debounce ────────────────────────────────────────────────
        let btn_raw = enc_button.is_low().unwrap_or(false);
        if btn_raw == btn_raw_prev {
            btn_stable_ticks = btn_stable_ticks.saturating_add(1);
        } else {
            btn_stable_ticks = 0;
            btn_raw_prev = btn_raw;
        }
        let newly_stable = btn_stable_ticks == DEBOUNCE_TICKS;
        if newly_stable {
            btn_stable = btn_raw;
        }

        if btn_stable {
            btn_hold_ticks = btn_hold_ticks.saturating_add(1);
            // Long-press: fires once per hold
            if btn_hold_ticks >= LONG_PRESS_MS && !long_press_fired {
                long_press_fired = true;
                state.load_enabled = !state.load_enabled;
                if state.load_enabled {
                    state.elapsed_s = 0;
                    state.capacity_mah = 0;
                    state.capacity_acc_mas = 0;
                    state.discharge_complete = false;
                    state.demand_ma = 0;
                    if state.mode == ControlMode::Discharge {
                        discharge_curve.reset();
                    }
                } else {
                    // Stopping
                    if state.mode == ControlMode::Discharge {
                        discharge_curve.finalize(state.capacity_mah);
                        state.discharge_complete = true;
                    }
                    state.demand_ma = 0;
                }
            }
        } else if newly_stable {
            // Button just released
            if btn_hold_ticks > 0
                && btn_hold_ticks < LONG_PRESS_MS
                && !long_press_fired
                && !state.load_enabled
            {
                // Short press (load must be OFF) → cycle operating mode
                state.mode = state.mode.next();
                state.setpoint = state.mode.default_setpoint();
            }
            btn_hold_ticks = 0;
            long_press_fired = false;
        }

        // ── 3. Load enable / disable ──────────────────────────────────────────
        if state.load_enabled {
            disable_pin.set_low().unwrap();
        } else {
            disable_pin.set_high().unwrap();
            let _ = channel.set_duty_cycle(0);
        }

        // ── 4. ADC sampling (every ADC_PERIOD ms) ────────────────────────────
        if loop_tick % ADC_PERIOD == 0 {
            // Voltage: V_actual = (raw × 3300 × VOLT_DIV_RATIO) / 4096  (mV)
            let v_raw: u16 = adc.read(&mut voltage_adc_pin).unwrap_or(0);
            state.voltage_mv =
                (v_raw as u32 * 3_300 * VOLT_DIV_RATIO) / 4_096;

            // Current: I_mA = (raw × 3300 × 1000) / (4096 × CURR_MV_PER_A)
            let i_raw: u16 = adc.read(&mut current_adc_pin).unwrap_or(0);
            state.current_ma =
                (i_raw as u32 * 3_300 * 1_000) / (4_096 * CURR_MV_PER_A);

            state.power_mw = (state.voltage_mv * state.current_ma) / 1_000;
        }

        // ── 5. Control loop ───────────────────────────────────────────────────
        if state.load_enabled {
            state.demand_ma = compute_current_ma(
                state.mode,
                state.setpoint,
                state.voltage_mv,
                state.demand_ma,
            );

            // Under-voltage cutoff (Discharge mode only)
            if state.mode == ControlMode::Discharge
                && state.voltage_mv > 0
                && state.voltage_mv < state.cutoff_mv
            {
                state.load_enabled = false;
                state.discharge_complete = true;
                discharge_curve.finalize(state.capacity_mah);
                state.demand_ma = 0;
            }

            let dac = current_to_dac(state.demand_ma, max_duty);
            let _ = channel.set_duty_cycle(dac);

            // Capacity integration + discharge recording (every 1 s).
            // Each loop iteration takes LOOP_PERIOD_US (= 1 ms), so
            // RECORD_PERIOD (= 1 000) iterations corresponds to exactly 1 second.
            if loop_tick % RECORD_PERIOD == 0 {
                // Accumulate mAs: add current_mA × 1 s per recording interval.
                state.capacity_acc_mas =
                    state.capacity_acc_mas.saturating_add(state.current_ma);
                state.capacity_mah = state.capacity_acc_mas / 3_600;
                state.elapsed_s = state.elapsed_s.saturating_add(1);

                if state.mode == ControlMode::Discharge {
                    discharge_curve.record(
                        state.elapsed_s.min(u16::MAX as u32) as u16,
                        state.voltage_mv,
                        state.current_ma,
                    );
                }
            }
        }

        // ── 6. Display refresh (every DISPLAY_PERIOD ms) ─────────────────────
        if loop_tick % DISPLAY_PERIOD == 0 {
            render_display(&mut display, &state, discharge_curve);
        }

        loop_tick = loop_tick.wrapping_add(1);
        delay.delay_us(LOOP_PERIOD_US);
    }
}

// ─── Display rendering ────────────────────────────────────────────────────────

/// Render the full UI onto the SSD1306 128×64 OLED.
///
/// Screen layout (using FONT_6X10):
/// ```text
///  Row 1  y=0   │ CC  [ ON  ]                  │
///  ─────────────────────────────────────────────
///  Row 2  y=13  │ SET   1.500 A                 │
///  Row 3  y=26  │ V11.23V  I 1.50A              │
///  Row 4  y=38  │ P16.88W  R 7.49O              │
///  ─────────────────────────────────────────────
///  Row 5  y=53  │ 0:05:23  1234mAh              │
/// ```
fn render_display<DI, SIZE>(
    display: &mut Ssd1306<DI, SIZE, ssd1306::mode::BufferedGraphicsMode<SIZE>>,
    state: &AppState,
    discharge: &DischargeCurve,
) where
    DI: ssd1306::prelude::WriteOnlyDataCommand,
    SIZE: ssd1306::prelude::DisplaySize,
    Ssd1306<DI, SIZE, ssd1306::mode::BufferedGraphicsMode<SIZE>>:
        DrawTarget<Color = BinaryColor>,
    <Ssd1306<DI, SIZE, ssd1306::mode::BufferedGraphicsMode<SIZE>> as DrawTarget>::Error:
        core::fmt::Debug,
{
    display.clear(BinaryColor::Off).unwrap();

    let style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();
    let line_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);

    // ── Row 1: mode + load status ─────────────────────────────────────────────
    {
        let mut buf: String<22> = String::new();
        let status = if state.load_enabled { "ON " } else { "OFF" };
        let _ = write!(buf, "{:<3}  [ {} ]", state.mode.short_name(), status);
        Text::with_baseline(buf.as_str(), Point::new(0, 0), style, Baseline::Top)
            .draw(display)
            .unwrap();
    }

    // Thin separator
    Line::new(Point::new(0, 11), Point::new(127, 11))
        .into_styled(line_style)
        .draw(display)
        .unwrap();

    // ── Row 2: setpoint ───────────────────────────────────────────────────────
    {
        let mut vbuf: String<10> = String::new();
        fmt_milli(&mut vbuf, state.setpoint, 3);
        let mut row: String<22> = String::new();
        let _ = write!(row, "SET {:>7} {}", vbuf.as_str(), state.mode.unit());
        Text::with_baseline(row.as_str(), Point::new(0, 13), style, Baseline::Top)
            .draw(display)
            .unwrap();
    }

    // ── Row 3: voltage + current ──────────────────────────────────────────────
    {
        let mut vbuf: String<8> = String::new();
        fmt_milli(&mut vbuf, state.voltage_mv, 2);
        let mut ibuf: String<8> = String::new();
        fmt_milli(&mut ibuf, state.current_ma, 3);
        let mut row: String<22> = String::new();
        let _ = write!(row, "V{}V I{}A", vbuf.as_str(), ibuf.as_str());
        Text::with_baseline(row.as_str(), Point::new(0, 26), style, Baseline::Top)
            .draw(display)
            .unwrap();
    }

    // ── Row 4: power + resistance ─────────────────────────────────────────────
    {
        let resistance_mohm = if state.current_ma > 0 {
            state.voltage_mv * 1_000 / state.current_ma
        } else {
            0
        };
        let mut pbuf: String<8> = String::new();
        fmt_milli(&mut pbuf, state.power_mw, 2);
        let mut rbuf: String<8> = String::new();
        fmt_milli(&mut rbuf, resistance_mohm, 2);
        let mut row: String<22> = String::new();
        let _ = write!(row, "P{}W R{}O", pbuf.as_str(), rbuf.as_str());
        Text::with_baseline(row.as_str(), Point::new(0, 38), style, Baseline::Top)
            .draw(display)
            .unwrap();
    }

    // Thin separator
    Line::new(Point::new(0, 50), Point::new(127, 50))
        .into_styled(line_style)
        .draw(display)
        .unwrap();

    // ── Row 5: elapsed time + capacity  (or discharge summary) ───────────────
    {
        let s = state.elapsed_s % 60;
        let m = (state.elapsed_s / 60) % 60;
        let h = state.elapsed_s / 3_600;
        let mut row: String<22> = String::new();
        if state.mode == ControlMode::Discharge && state.discharge_complete {
            let _ = write!(row, "DONE  {}mAh", discharge.total_capacity_mah);
        } else {
            let _ = write!(row, "{:01}:{:02}:{:02}  {}mAh", h, m, s, state.capacity_mah);
        }
        Text::with_baseline(row.as_str(), Point::new(0, 52), style, Baseline::Top)
            .draw(display)
            .unwrap();
    }

    // ── Discharge voltage bar (Discharge mode, running) ───────────────────────
    if state.mode == ControlMode::Discharge
        && state.load_enabled
        && discharge.start_voltage_mv > 0
        && state.voltage_mv > state.cutoff_mv
    {
        let start_mv = discharge.start_voltage_mv as u32;
        let cutoff_mv = state.cutoff_mv;
        let current_mv = state.voltage_mv;
        // Map [cutoff_mv .. start_mv] → pixel width [0 .. 127]
        let bar_w = if start_mv > cutoff_mv {
            (current_mv.saturating_sub(cutoff_mv) * 127 / (start_mv - cutoff_mv))
                .min(127)
        } else {
            0
        };
        if bar_w > 0 {
            Rectangle::new(Point::new(0, VOLTAGE_BAR_Y), Size::new(bar_w, 2))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(display)
                .unwrap();
        }
    }

    display.flush().unwrap();
}

// ─── Number formatting helper ─────────────────────────────────────────────────

/// Format a milli-unit value (e.g. 11_234 mV) as a fixed-point string.
///
/// * `frac_digits = 3` → "11.234"
/// * `frac_digits = 2` → "11.23"
fn fmt_milli<const N: usize>(buf: &mut String<N>, milli: u32, frac_digits: u32) {
    let int = milli / 1_000;
    let frac = milli % 1_000;
    match frac_digits {
        2 => { let _ = write!(buf, "{}.{:02}", int, frac / 10); }
        3 => { let _ = write!(buf, "{}.{:03}", int, frac); }
        _ => { let _ = write!(buf, "{}", int); }
    }
}
