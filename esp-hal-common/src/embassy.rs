use core::{
    ptr,
    sync::atomic::{compiler_fence, Ordering},
}; // TODO use atomic polyfill here?

use embassy::interrupt::{Interrupt, InterruptExt};

use crate::{disable, enable, pac::Peripherals, set_priority, Cpu, CpuInterrupt, Priority};

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
    let peripherals = Peripherals::take().unwrap();

    // TODO implement the system timer
    // TODO initialize it here

    peripherals
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
