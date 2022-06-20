use core::{cell::Cell, ptr}; // TODO use atomic polyfill here?

use critical_section::CriticalSection;
use embassy::{
    blocking_mutex::{raw::CriticalSectionRawMutex, CriticalSectionMutex as Mutex},
    time::driver::{AlarmHandle, Driver},
};

use crate::{
    interrupt,
    pac,
    pac::Peripherals,
    systimer::{Alarm, SystemTimer, Target},
    Cpu,
    CpuInterrupt,
    Priority,
};

pub fn init() -> Peripherals {
    // Do this first, so that it panics if user is calling `init` a second time
    // before doing anything important.
    let peripherals = Peripherals::take().unwrap(); // TODO make new `Peripherals` without SYSTIMER as we use it in embassy

    // TODO implement the system timer
    // TODO initialize it here
    EmbassyTimer::init();

    peripherals
}

const ALARM_COUNT: usize = 3;

pub struct AlarmState {
    pub timestamp: Cell<u64>,

    // This is really a Option<(fn(*mut ()), *mut ())>
    // but fn pointers aren't allowed in const yet
    pub callback: Cell<*const ()>,
    pub ctx: Cell<*mut ()>,
    pub allocated: Cell<bool>,
}

unsafe impl Send for AlarmState {}

impl AlarmState {
    pub const fn new() -> Self {
        Self {
            timestamp: Cell::new(u64::MAX),
            callback: Cell::new(ptr::null()),
            ctx: Cell::new(ptr::null_mut()),
            allocated: Cell::new(false),
        }
    }
}

pub struct EmbassyTimer {
    pub alarms: Mutex<[AlarmState; ALARM_COUNT]>,
    pub alarm0: Alarm<Target, 0>,
    pub alarm1: Alarm<Target, 1>,
    pub alarm2: Alarm<Target, 2>,
}

const ALARM_STATE_NONE: AlarmState = AlarmState::new();

embassy::time_driver_impl!(static DRIVER: EmbassyTimer = EmbassyTimer {
    alarms: Mutex::const_new(CriticalSectionRawMutex::new(), [ALARM_STATE_NONE; ALARM_COUNT]),
    alarm0: unsafe { Alarm::<_, 0>::conjure() },
    alarm1: unsafe { Alarm::<_, 1>::conjure() },
    alarm2: unsafe { Alarm::<_, 2>::conjure() },
});

impl EmbassyTimer {
    fn trigger_alarm(&self, n: usize, cs: CriticalSection) {
        let alarm = &self.alarms.borrow(cs)[n];
        // safety:
        // - we can ignore the possiblity of `f` being unset (null) because of the
        //   safety contract of `allocate_alarm`.
        // - other than that we only store valid function pointers into alarm.callback
        let f: fn(*mut ()) = unsafe { core::mem::transmute(alarm.callback.get()) };
        f(alarm.ctx.get());
    }

    fn on_interrupt(&self, id: u8) {
        match id {
            0 => self.alarm0.clear_interrupt(),
            1 => self.alarm1.clear_interrupt(),
            2 => self.alarm2.clear_interrupt(),
            _ => panic!(),
        };
        critical_section::with(|cs| {
            self.trigger_alarm(id as usize, cs);
        })
    }

    pub fn init() {
        interrupt::enable(
            Cpu::ProCpu,
            pac::Interrupt::SYSTIMER_TARGET0,
            interrupt::CpuInterrupt::Interrupt1,
        );
        interrupt::enable(
            Cpu::ProCpu,
            pac::Interrupt::SYSTIMER_TARGET1,
            interrupt::CpuInterrupt::Interrupt2,
        );
        interrupt::enable(
            Cpu::ProCpu,
            pac::Interrupt::SYSTIMER_TARGET2,
            interrupt::CpuInterrupt::Interrupt3,
        );
        interrupt::set_kind(
            Cpu::ProCpu,
            interrupt::CpuInterrupt::Interrupt1,
            interrupt::InterruptKind::Level,
        );
        interrupt::set_kind(
            Cpu::ProCpu,
            interrupt::CpuInterrupt::Interrupt2,
            interrupt::InterruptKind::Level,
        );
        interrupt::set_kind(
            Cpu::ProCpu,
            interrupt::CpuInterrupt::Interrupt3,
            interrupt::InterruptKind::Level,
        );
        interrupt::set_priority(
            Cpu::ProCpu,
            interrupt::CpuInterrupt::Interrupt1,
            interrupt::Priority::Priority1,
        );
        interrupt::set_priority(
            Cpu::ProCpu,
            interrupt::CpuInterrupt::Interrupt2,
            interrupt::Priority::Priority1,
        );
        interrupt::set_priority(
            Cpu::ProCpu,
            interrupt::CpuInterrupt::Interrupt3,
            interrupt::Priority::Priority1,
        );

        #[no_mangle]
        pub fn interrupt1() {
            DRIVER.on_interrupt(0);
        }
        #[no_mangle]
        pub fn interrupt2() {
            DRIVER.on_interrupt(1);
        }
        #[no_mangle]
        pub fn interrupt3() {
            DRIVER.on_interrupt(2);
        }
    }
}

impl Driver for EmbassyTimer {
    fn now(&self) -> u64 {
        SystemTimer::now()
    }

    unsafe fn allocate_alarm(&self) -> Option<AlarmHandle> {
        return critical_section::with(|_cs| {
            let alarms = self.alarms.borrow(_cs);
            for i in 0..ALARM_COUNT {
                let c = alarms.get_unchecked(i);
                if !c.allocated.get() {
                    // set alarm so it is not overwritten
                    c.allocated.set(true);
                    return Option::Some(AlarmHandle::new(i as u8));
                }
            }
            return Option::None;
        });
    }

    fn set_alarm_callback(
        &self,
        alarm: embassy::time::driver::AlarmHandle,
        callback: fn(*mut ()),
        ctx: *mut (),
    ) {
        critical_section::with(|cs| {
            let alarm = unsafe { self.alarms.borrow(cs).get_unchecked(alarm.id() as usize) };
            alarm.callback.set(callback as *const ());
            alarm.ctx.set(ctx);
        })
    }

    fn set_alarm(&self, alarm: embassy::time::driver::AlarmHandle, timestamp: u64) {
        critical_section::with(|cs| {
            let now = self.now();
            if timestamp < now {
                self.trigger_alarm(alarm.id() as usize, cs);
                return;
            }
            let alarm_state = unsafe { self.alarms.borrow(cs).get_unchecked(alarm.id() as usize) };
            alarm_state.timestamp.set(timestamp);
            match alarm.id() {
                0 => {
                    self.alarm0.set_target(timestamp);
                    self.alarm0.enable_interrupt();
                }
                1 => {
                    self.alarm1.set_target(timestamp);
                    self.alarm1.enable_interrupt();
                }
                2 => {
                    self.alarm2.set_target(timestamp);
                    self.alarm2.enable_interrupt();
                }
                _ => panic!(),
            }
        })
    }
}

mod esp32c3_interrupt_controller {
    #![allow(unused)]
    use super::*;
    // esp32c3 specific items to be generalised one day

    pub fn is_pending(cpu_interrupt_number: CpuInterrupt) -> bool {
        unsafe {
            let intr = &*crate::pac::INTERRUPT_CORE0::ptr();
            let b = intr.cpu_int_eip_status.read().bits();
            let ans = b & (1 << cpu_interrupt_number as isize);
            ans != 0
        }
    }

    pub fn get_priority(which: CpuInterrupt) -> Priority {
        unsafe {
            let intr = &*crate::pac::INTERRUPT_CORE0::ptr();
            let cpu_interrupt_number = which as isize;
            let intr_prio_base = intr.cpu_int_pri_0.as_ptr();

            let prio = intr_prio_base
                .offset(cpu_interrupt_number as isize)
                .read_volatile();

            (prio as u8).into()
        }
    }

    pub fn is_enabled(cpu_interrupt_number: CpuInterrupt) -> bool {
        unsafe {
            let intr = &*crate::pac::INTERRUPT_CORE0::ptr();
            let b = intr.cpu_int_enable.read().bits();
            let ans = b & (1 << cpu_interrupt_number as isize);
            ans != 0
        }
    }

    pub fn pend(_interrupt: crate::pac::Interrupt) {
        todo!("pend impl needed")
        // unsafe {
        //     // TODO set the interrupt pending in the status some how
        //     // todo!();
        //     // possible workaround, store pending interrupt in atomic global
        //     // catch the software interrupt and check the global?

        //     SOFT_PEND = Some(interrupt); // TODO what if this is some
        // already?

        //     // trigger an interrupt via the software interrupt mechanism
        //     let system = &*crate::pac::SYSTEM::ptr();
        //     system
        //         .cpu_intr_from_cpu_0
        //         .modify(|_, w| w.cpu_intr_from_cpu_0().set_bit());
        // }
    }

    pub fn unpend(_interrupt: crate::pac::Interrupt, _cpu: CpuInterrupt) {
        // unsafe {
        //     // unpend an awaiting software interrupt
        //     let system = &*crate::pac::SYSTEM::ptr();
        //     system
        //         .cpu_intr_from_cpu_0
        //         .modify(|_, w| w.cpu_intr_from_cpu_0().clear_bit());
        //     clear(Cpu::ProCpu, cpu)
        // }
        todo!("unpend impl needed")
    }
}
