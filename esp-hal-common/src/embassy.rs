use core::{
    ptr,
    sync::atomic::{compiler_fence, Ordering},
}; // TODO use atomic polyfill here?

use embassy::interrupt::{Interrupt, InterruptExt};

use crate::{interrupt, CpuInterrupt, Priority};

macro_rules! embassy_interrupt {
    (
        $(($struct:ident, $cpu:ident, $handler:tt)),*
    ) => {
        $(
        pub struct $struct(CpuInterrupt);

        unsafe impl Interrupt for $struct {
            type Priority = Priority;

            fn number(&self) -> u16 {
                self.0 as _
            }

            unsafe fn steal() -> Self {
                $struct(CpuInterrupt::$cpu)
            }

            unsafe fn __handler(&self) -> &'static embassy::interrupt::Handler {
                #[export_name = $handler]
                static HANDLER: ::embassy::interrupt::Handler = ::embassy::interrupt::Handler::new();
                &HANDLER
            }
        }

        unsafe impl ::embassy::util::Unborrow for $struct {
            type Target = $struct;
            unsafe fn unborrow(self) -> $struct {
                self
            }
        }
        )+
    };
}

// Macro based on `interrupt_declare` in embassy
// TODO the rest
// TODO this does not bind a cpu interrupt to a peripheral
embassy_interrupt!(
    (EmbassyInterrupt1, Interrupt1, "EMBASSYINTERRUPT1"),
    (EmbassyInterrupt2, Interrupt2, "EMBASSYINTERRUPT2"),
    (EmbassyInterrupt3, Interrupt3, "EMBASSYINTERRUPT3"),
    (EmbassyInterrupt4, Interrupt4, "EMBASSYINTERRUPT4"),
    (EmbassyInterrupt5, Interrupt5, "EMBASSYINTERRUPT5"),
    (EmbassyInterrupt6, Interrupt6, "EMBASSYINTERRUPT6"),
    (EmbassyInterrupt7, Interrupt7, "EMBASSYINTERRUPT7"),
    (EmbassyInterrupt8, Interrupt8, "EMBASSYINTERRUPT8"),
    (EmbassyInterrupt9, Interrupt9, "EMBASSYINTERRUPT9"),
    (EmbassyInterrupt10, Interrupt10, "EMBASSYINTERRUPT10"),
    (EmbassyInterrupt11, Interrupt11, "EMBASSYINTERRUPT11"),
    (EmbassyInterrupt12, Interrupt12, "EMBASSYINTERRUPT12"),
    (EmbassyInterrupt13, Interrupt13, "EMBASSYINTERRUPT13")
);

impl From<u8> for Priority {
    fn from(p: u8) -> Self {
        p.into()
    }
}

impl Into<u8> for Priority {
    fn into(self) -> u8 {
        self as _
    }
}

impl<T: Interrupt + ?Sized> InterruptExt for T {
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
        todo!()
    }

    fn disable(&self) {
        todo!()
    }

    fn is_active(&self) -> bool {
        todo!()
    }

    fn is_enabled(&self) -> bool {
        todo!()
    }

    fn is_pending(&self) -> bool {
        todo!()
    }

    fn pend(&self) {
        todo!()
    }

    fn unpend(&self) {
        todo!()
    }

    fn get_priority(&self) -> Self::Priority {
        todo!()
    }

    fn set_priority(&self, prio: Self::Priority) {
        todo!()
    }
}
