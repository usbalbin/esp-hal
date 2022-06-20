use self::time_driver::EmbassyTimer;
use crate::pac::Peripherals;

mod time_driver;

pub fn init() -> Peripherals {
    // Do this first, so that it panics if user is calling `init` a second time
    // before doing anything important.
    let peripherals = Peripherals::take().unwrap(); // TODO make new `Peripherals` without SYSTIMER as we use it in embassy

    // TODO implement the system timer
    // TODO initialize it here
    EmbassyTimer::init();

    peripherals
}
