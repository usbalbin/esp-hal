#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

use core::fmt::Write;

use esp32s2_hal::{pac::Peripherals, prelude::*, RtcCntl, Serial, Timer};
use nb::block;
use panic_halt as _;
// TODO why do I need extern crate too?
use xtensa_atomic_emulation_trap as _;
extern crate xtensa_atomic_emulation_trap;

use xtensa_lx_rt::entry;

#[entry]
fn main() -> ! {
    let peripherals = Peripherals::take().unwrap();

    let mut timer0 = Timer::new(peripherals.TIMG0);
    let mut rtc_cntl = RtcCntl::new(peripherals.RTC_CNTL);
    let mut serial0 = Serial::new(peripherals.UART0).unwrap();

    // Disable MWDT and RWDT (Watchdog) flash boot protection
    timer0.disable();
    rtc_cntl.set_wdt_global_enable(false);

    timer0.start(40_000_000u64);

    loop {
        writeln!(serial0, "Hello world!").unwrap();
        block!(timer0.wait()).unwrap();

        use core::sync::atomic::AtomicUsize;
        let x = AtomicUsize::new(0);

        let old = x.compare_and_swap(0, 12, core::sync::atomic::Ordering::Release);

        writeln!(serial0, "Old: {}", old).unwrap();

        writeln!(
            serial0,
            "Current: {}",
            x.load(core::sync::atomic::Ordering::SeqCst)
        )
        .unwrap();

        let old = x.compare_and_swap(12, 13, core::sync::atomic::Ordering::Release);

        writeln!(serial0, "Old2: {}", old).unwrap();

        writeln!(
            serial0,
            "Current2: {}",
            x.load(core::sync::atomic::Ordering::SeqCst)
        )
        .unwrap();

        writeln!(serial0).ok();
    }
}
