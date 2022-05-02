#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

use core::fmt::Write;

use esp32s2_hal::{pac::Peripherals, prelude::*, RtcCntl, Serial, Timer};
use nb::block;
use panic_halt as _;
use xtensa_atomic_emulation_trap as _;
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

    xtensa_atomic_emulation_trap::test_print();

    loop {
        writeln!(serial0, "Hello world!").unwrap();
        block!(timer0.wait()).unwrap();

        writeln!(serial0, "About to trigger exception").unwrap();
        let x = 4;
        unsafe { core::arch::asm!("wsr {}, SCOMPARE1", in(reg) x) };
    }
}
