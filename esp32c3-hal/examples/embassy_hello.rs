#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use core::{cell::RefCell, fmt::Write};

use embassy::{
    self,
    blocking_mutex::CriticalSectionMutex as Mutex,
    executor::{Executor, InterruptExecutor},
    time::{Duration, Timer},
    util::Forever,
};
use esp32c3_hal::{pac::UART0, prelude::*, RtcCntl, Serial, Timer as EspTimer};
use panic_halt as _;

#[embassy::task]
async fn run_low() {
    loop {
        esp_println::println!("Hello world from embassy on an esp32c3!");
        Timer::after(Duration::from_millis(1000)).await;
    }
}

#[embassy::task]
async fn run2() {
    loop {
        esp_println::println!("Bing!");
        Timer::after(Duration::from_millis(3000)).await;
    }
}

static EXECUTOR_LOW: Forever<Executor> = Forever::new();

#[riscv_rt::entry]
fn main() -> ! {
    let p = esp32c3_hal::embassy::init();

    let mut rtc_cntl = RtcCntl::new(p.RTC_CNTL);
    let mut timer0 = EspTimer::new(p.TIMG0);
    let mut timer1 = EspTimer::new(p.TIMG1);

    rtc_cntl.set_super_wdt_enable(false);
    rtc_cntl.set_wdt_enable(false);
    timer0.disable();
    timer1.disable();

    let executor = EXECUTOR_LOW.put(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(run_low()).ok();
        spawner.spawn(run2()).ok();
    });

    loop {}
}
