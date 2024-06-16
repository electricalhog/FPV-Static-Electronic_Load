#![no_std]
#![no_main]

use cortex_m_rt::entry;
use embedded_hal::{digital::v2::InputPin, timer::CountDown};
use rp2040_hal::{clocks::init_clocks_and_plls, gpio::{bank0::{Gpio8, Gpio9}, Input, Pin, PullUp}, pac::{self, Interrupt, NVIC}, sio::Sio, timer::{self, Timer}, watchdog::Watchdog, Clock};
use panic_halt as _;
use rotary_encoder_embedded::{Direction, RotaryEncoder};


const TIMER_FREQ_HZ: u32 = 900;

#[entry]
fn main() -> ! {
    // Grab our singleton objects
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();

    // External high-speed crystal on the pico board is 12Mhz
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let clocks = init_clocks_and_plls(
        12_000_000,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let sio = Sio::new(pac.SIO);
    let pins = rp2040_hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // Configure rotary encoder pins
    let pin_a: Pin<Gpio8, Input<PullUp>> = pins.gpio8.into_pull_up_input();
    let pin_b: Pin<Gpio9, Input<PullUp>> = pins.gpio9.into_pull_up_input();

    // Create the rotary encoder instance
    let mut encoder = RotaryEncoder::new(pin_a, pin_b).into_standard_mode();

    let mut position: i32 = 0;

    let mut timer = Timer::new(pac.TIMER, &mut pac.RESETS);
        
    // Calculate timer count for 900Hz
    let timer_freq_hz = clocks.system_clock.freq().0;
    let timer_count = timer_freq_hz / TIMER_FREQ_HZ;
    
    // Set timer count down
    timer::CountDown::start(&mut timer, timer_count);
    
     // Enable the timer interrupt in the NVIC
     unsafe {
        NVIC::unmask(Interrupt::TIMER_IRQ_0);
    }


    loop {
    }
}


#[Interrupt]
fn TIMER_IRQ_0() {
    static mut TIMER: Option<Timer> = None;

    // Initialize the static TIMER variable on the first interrupt call
    if unsafe { TIMER.is_none() } {
        let pac = unsafe { pac::Peripherals::steal() };
        let mut timer = Timer::new(pac.TIMER, &mut pac.RESETS);
        
        // Set timer countdown
        let clocks = unsafe { pac::CLOCKS.steal() };
        let timer_freq_hz = clocks.system_clock.freq().0;
        let timer_count = timer_freq_hz / TIMER_FREQ_HZ;
        
        timer.start(timer_count);
        TIMER.replace(timer);
    }

    if let Some(timer) = TIMER.as_mut() {
        // Clear the interrupt flag
        timer.clear_interrupt(timer::Alarm::Alarm0);

        // Your interrupt handling code here
        // For example, you could toggle an LED or increment a counter
        match encoder.update() {
            Direction::Clockwise => {
                position += 1;
                // Handle clockwise rotation (e.g., increment position)
            }
            Direction::Anticlockwise => {
                position -= 1;
                // Handle counterclockwise rotation (e.g., decrement position)
            }
            Direction::None => {
                // Do nothing
            }
        }
    }
}