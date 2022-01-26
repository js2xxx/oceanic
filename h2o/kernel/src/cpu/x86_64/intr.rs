pub(super) mod def;

pub use self::def::{ExVec, ALLOC_VEC};
use crate::{
    cpu::{arch::seg::ndt::USR_CODE_X64, time::Instant},
    sched::{
        task::{self, ctx::arch::Frame},
        SCHED,
    },
};

/// Generic exception handler.
unsafe fn exception(frame_ptr: *mut Frame, vec: def::ExVec) {
    use def::ExVec::*;

    let frame = &mut *frame_ptr;
    match vec {
        PageFault if crate::mem::space::page_fault(&mut *frame_ptr, frame.errc_vec) => return,
        _ => {}
    }

    match SCHED.with_current(|cur| Ok(cur.tid().ty())) {
        Ok(task::Type::User) if frame.cs == USR_CODE_X64.into_val().into() => {
            if !task::dispatch_exception(frame, vec) {
                // #[cfg(debug_assertions)]
                // let _ = SCHED.with_current(|cur| {
                //     log::warn!("Unhandled exception from task {}.", cur.tid().raw());
                //     Ok(())
                // });
                // Kill the fucking task.
                SCHED.exit_current(solvent::Error::EFAULT.into_retval())
            }
            // unreachable!()
        }
        _ => {}
    }

    // No more available remedies. Die.
    log::error!("{:?}", vec);

    frame.dump(if vec == PageFault {
        Frame::ERRC_PF
    } else {
        Frame::ERRC
    });

    archop::halt_loop(Some(false));
}

/// # Safety
///
/// This function must only be called from its assembly routine `rout_XX`.
#[no_mangle]
unsafe extern "C" fn common_interrupt(frame: *mut Frame) {
    let vec = unsafe { &*frame }.errc_vec as u16;
    log::warn!("Unhandled interrupt #{}", vec);
    crate::sched::SCHED.tick(Instant::now());
}
