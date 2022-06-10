use core::{
    cell::Cell,
    ptr,
    sync::atomic::{compiler_fence, Ordering},
}; // TODO use atomic polyfill here?

use critical_section::CriticalSection;
use embassy::{
    blocking_mutex::{raw::CriticalSectionRawMutex, CriticalSectionMutex as Mutex},
    interrupt::{Interrupt, InterruptExt},
    time::driver::{AlarmHandle, Driver},
};

use crate::{
    disable,
    enable,
    interrupt,
    pac,
    pac::Peripherals,
    set_priority,
    systimer::{Alarm, SystemTimer, Target},
    Cpu,
    CpuInterrupt,
    Priority,
};

pub unsafe trait PeripheralInterrupt {
    fn peripheral_number(&self) -> u16;
}

macro_rules! embassy_interrupt {
    (
        $(($struct:ident, $interrupt:ident, $cpu:ident, $handler:tt)),*
    ) => {
        $(
        pub struct $struct(crate::pac::Interrupt, CpuInterrupt);

        unsafe impl Interrupt for $struct {
            type Priority = Priority;

            fn number(&self) -> u16 {
                self.1 as _
            }

            unsafe fn steal() -> Self {
                $struct(crate::pac::Interrupt::$interrupt, CpuInterrupt::$cpu)
            }

            unsafe fn __handler(&self) -> &'static embassy::interrupt::Handler {
                #[export_name = $handler]
                static HANDLER: ::embassy::interrupt::Handler = ::embassy::interrupt::Handler::new();
                &HANDLER
            }
        }

        unsafe impl PeripheralInterrupt for $struct {
            fn peripheral_number(&self) -> u16 {
                self.0 as _
            }
        }

        unsafe impl ::embassy::util::Unborrow for $struct {
            type Target = $struct;
            unsafe fn unborrow(self) -> $struct {
                self
            }
        }

        impl InterruptExt for $struct {
            fn set_handler(&self, func: unsafe fn(*mut ())) {
                compiler_fence(Ordering::SeqCst);
                let handler = unsafe { self.__handler() };
                handler.func.store(func as *mut (), Ordering::Relaxed);
                compiler_fence(Ordering::SeqCst);
            }

            fn remove_handler(&self) {
                compiler_fence(Ordering::SeqCst);
                let handler = unsafe { self.__handler() };
                handler.func.store(ptr::null_mut(), Ordering::Relaxed);
                compiler_fence(Ordering::SeqCst);
            }

            fn set_handler_context(&self, ctx: *mut ()) {
                let handler = unsafe { self.__handler() };
                handler.ctx.store(ctx, Ordering::Relaxed);
            }

            fn enable(&self) {
                compiler_fence(Ordering::SeqCst);
                let s = unsafe { Self::steal() };
                enable(
                    Cpu::ProCpu, // TODO remove hardcode
                    s.0,
                    s.1,
                );
            }

            fn disable(&self) {
                let s = unsafe { Self::steal() };
                disable(
                    Cpu::ProCpu, // TODO remove hardcode
                    s.0,
                );
                compiler_fence(Ordering::SeqCst);
            }

            fn is_active(&self) -> bool {
                let cause = riscv::register::mcause::read().cause();
                matches!(cause, riscv::register::mcause::Trap::Interrupt(_))
            }

            fn is_enabled(&self) -> bool {
                // TODO check that peripheral interrupt is installed at cpu slot
                esp32c3_interrupt_controller::is_enabled(self.1)
            }

            fn is_pending(&self) -> bool {
                esp32c3_interrupt_controller::is_pending(self.1)
            }

            fn pend(&self) {
                esp32c3_interrupt_controller::pend(self.0)
            }

            fn unpend(&self) {
                esp32c3_interrupt_controller::unpend(self.0, self.1)
            }

            fn get_priority(&self) -> Self::Priority {
                esp32c3_interrupt_controller::get_priority(self.1)
            }

            fn set_priority(&self, prio: Self::Priority) {
                let s = unsafe { Self::steal() };
                set_priority(Cpu::ProCpu, s.1, prio)
            }
        }
        )+
    };
}

// TODO this needs to be a proc macro that can take dynamic input from the user
embassy_interrupt!((GpioInterrupt, GPIO, Interrupt1, "__ESP_HAL_GPIOINTERRUPT"));

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
