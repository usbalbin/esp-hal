//! Delay driver
//!
//! Implement the `DelayMs` and `DelayUs` traits from [embedded-hal].
//!
//! [embedded-hal]: https://docs.rs/embedded-hal/latest/embedded_hal/

use embedded_hal::blocking::delay::{DelayMs, DelayUs};

pub use self::delay::Delay;

impl<T, const N: u8> DelayMs<T> for Delay<N>
where
    T: Into<u32>,
{
    fn delay_ms(&mut self, ms: T) {
        for _ in 0..ms.into() {
            self.delay_us(1000u32);
        }
    }
}

impl<T, const N: u8> DelayUs<T> for Delay<N>
where
    T: Into<u32>,
{
    fn delay_us(&mut self, us: T) {
        self.delay(us.into());
    }
}

#[cfg(feature = "esp32c3")]
mod delay {
    use fugit::HertzU64;

    use crate::{
        clock::Clocks,
        systimer::{Alarm, SystemTimer, Target},
    };

    /// Uses the `SYSTIMER` peripheral for counting clock cycles, as
    /// unfortunately the ESP32-C3 does NOT implement the `mcycle` CSR, which is
    /// how we would normally do this.
    pub struct Delay<const N: u8> {
        systimer_alarm: Alarm<Target, N>,
        freq: HertzU64,
    }

    impl<const N: u8> Delay<N> {
        /// Create a new Delay instance
        pub fn new(systimer_alarm: Alarm<Target, N>, clocks: &Clocks) -> Self {
            // The counters and comparators are driven using `XTAL_CLK`. The average clock
            // frequency is fXTAL_CLK/2.5, which is 16 MHz. The timer counting is
            // incremented by 1/16 μs on each `CNT_CLK` cycle.

            Self {
                systimer_alarm,
                freq: HertzU64::MHz((clocks.xtal_clock.to_MHz() * 10 / 25) as u64),
            }
        }

        /// Return the raw interface to the underlying SYSTIMER instance
        pub fn free(self) -> Alarm<Target, N> {
            self.systimer_alarm
        }

        /// Delay for the specified number of microseconds
        pub fn delay(&self, us: u32) {
            let t0 = SystemTimer::now();
            let clocks = (us as u64 * self.freq.raw()) / HertzU64::MHz(1).raw();

            let target = t0 as u128 + clocks as u128;
            // check if the target exceeds the 52bit limit of the timer
            let target = if target > SystemTimer::BIT_MASK as u128 {
                target as u64 - SystemTimer::BIT_MASK
            } else {
                target as u64
            };

            self.systimer_alarm.set_target(target);
            self.systimer_alarm.wait();
        }
    }
}

#[cfg(not(feature = "esp32c3"))]
mod delay {

    use fugit::HertzU64;

    use crate::clock::Clocks;

    /// Delay driver
    ///
    /// Uses the built-in Xtensa timer from the `xtensa_lx` crate.
    pub struct Delay {
        freq: HertzU64,
    }

    impl Delay {
        /// Instantiate the `Delay` driver
        pub fn new(clocks: &Clocks) -> Self {
            Self {
                freq: HertzU64::MHz(clocks.cpu_clock.to_MHz() as u64),
            }
        }

        /// Delay for the specified number of microseconds
        pub fn delay(&self, us: u32) {
            let clocks = (us as u64 * self.freq.raw()) / HertzU64::MHz(1).raw();
            xtensa_lx::timer::delay(clocks as u32);
        }
    }
}
