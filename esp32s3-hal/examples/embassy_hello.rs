#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]


use embassy::{
    self,
    executor::{Executor},
    time::{Duration, Timer},
    util::Forever,
};
use esp32s3_hal::{prelude::*, RtcCntl, Timer as EspTimer};
use esp_backtrace as _;

const ENABLE_MASK: u32 = 1 << 19 | 1 << 0 | 1 << 23 ;

#[embassy::task]
async fn run_low() {
    loop {
        esp_println::println!("Hello world from embassy on an esp32s3!");
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

#[xtensa_lx_rt::entry]
fn main() -> ! {
    let p = esp32s3_hal::embassy::init();

    let mut rtc_cntl = RtcCntl::new(p.RTC_CNTL);
    let mut timer0 = EspTimer::new(p.TIMG0);

    // Disable MWDT and RWDT (Watchdog) flash boot protection
    timer0.disable();
    rtc_cntl.set_wdt_global_enable(false);

    esp_println::println!("About to enable interrupts");

    unsafe {
        xtensa_lx::interrupt::enable_mask(ENABLE_MASK);
    }

    let executor = EXECUTOR_LOW.put(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(run_low()).ok();
        spawner.spawn(run2()).ok();
    });
}


struct CriticalSection;
critical_section::custom_impl!(CriticalSection);

unsafe impl critical_section::Impl for CriticalSection {
    unsafe fn acquire() -> u8 {
        return xtensa_lx::interrupt::disable() as _;
    }

    unsafe fn release(token: u8) {
        if token != 0 {
            xtensa_lx::interrupt::enable_mask(ENABLE_MASK);
        }
    }
}