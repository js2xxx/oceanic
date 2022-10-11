use core::ptr;

use solvent::prelude::{Instant, SIG_READ};
use sv_call::{ipc::SIG_TIMER, *};

pub unsafe fn test() {
    let timer = sv_timer_new().into_res().expect("Failed to create timer");
    let disp = sv_disp_new(5)
        .into_res()
        .expect("Failed to create dispatcher");
    let key = sv_disp_push(disp, timer, true, SIG_TIMER, ptr::null())
        .into_res()
        .expect("Failed to set wait for timer");
    sv_timer_set(timer, 10000)
        .into_res()
        .expect("Failed to set timer");
    let time = Instant::now();
    sv_obj_wait(disp, u64::MAX, true, false, SIG_READ)
        .into_res()
        .expect("Failed to wait for dispatcher");
    let mut signal = 0;
    let k2 = sv_disp_pop(disp, &mut signal, ptr::null_mut())
        .into_res()
        .expect("Failed to wait for timer");
    assert_eq!(key, k2);
    log::debug!("Waiting for 10ms, actual passed {:?}", time.elapsed());
    sv_obj_drop(disp)
        .into_res()
        .expect("Failed to drop dispatcher");
    sv_obj_drop(timer).into_res().expect("Failed to drop timer");
}
