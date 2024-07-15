#![no_std]
#![no_main]

use cortex_m_rt as _;
use cortex_m::{self as _, interrupt::disable};
// Ensure we halt the program on panic (if we don't mention this crate it won't
// be linked)
use panic_halt as _;

// Alias for our HAL crate
use rp2040_hal as hal;

// Some traits we need
use embedded_hal::{digital::OutputPin, pwm::SetDutyCycle};
use rp2040_hal::clocks::Clock;

use defmt as _;

// A shorter alias for the Peripheral Access Crate, which provides low-level
// register access
use hal::pac;
use rotary_encoder_embedded::{Direction, RotaryEncoder};

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
/// Note: This boot block is not necessary when using a rp-hal based BSP
/// as the BSPs already perform this step.
#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

#[rp2040_hal::entry]
fn main() -> ! {
    // Grab our singleton objects
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();

    // External high-speed crystal on the pico board is 12Mhz
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
    // Configure rotary encoder pins
    let pin_a = pins.gpio8.into_pull_up_input();
    let pin_b = pins.gpio9.into_pull_up_input();

    // The delay object lets us wait for specified amounts of time (in
    // milliseconds)
    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());

    // Init PWM
    let mut pwm_slices = hal::pwm::Slices::new(pac.PWM, &mut pac.RESETS);

    // Configure PWM
    let pwm = &mut pwm_slices.pwm5;
    pwm.set_ph_correct();
    pwm.enable();

    // Output channel B on PWM5 to GPIO27
    let channel = &mut pwm.channel_b;
    channel.output_to(pins.gpio27);
    
    // Create the rotary encoder instance
    let mut encoder = RotaryEncoder::new(pin_a, pin_b).into_standard_mode();

    let mut position: u16 = 0;

    // Configure DISABLE control pin and set to enabled
    let mut disable_pin = pins.gpio6.into_push_pull_output();
    disable_pin.set_low().unwrap();

    // let mut timer = Timer::new(pac.TIMER, &mut pac.RESETS);
    //    
    // // Calculate timer count for 900Hz
    // let timer_count = Microseconds::new(1_000_000/TIMER_FREQ_HZ);
    // // Set timer count down
    // timer.count_down().start(timer_count);
    
    
    //  // Enable the timer interrupt in the NVIC
    //  unsafe {
    //     NVIC::unmask(Interrupt::TIMER_IRQ_0);
    // }


    loop {
        match encoder.update() {
            Direction::Clockwise => {
                if position < channel.max_duty_cycle(){
                    position += 1000; 
                }
                // Handle clockwise rotation (e.g., increment position)
            }
            Direction::Anticlockwise => {
                if position > 1000{
                    position -= 1000;
                }
                // Handle counterclockwise rotation (e.g., decrement position)
            }
            Direction::None => {
                // Do nothing
            }
        }

        let _ = channel.set_duty_cycle(position);

        delay.delay_us(1111);
    }
}
