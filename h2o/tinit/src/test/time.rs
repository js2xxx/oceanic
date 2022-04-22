use solvent::prelude::Instant;
use sv_call::{ipc::SIG_TIMER, *};

pub unsafe fn test() {
    let timer = sv_timer_new().into_res().expect("Failed to create timer");
    let waiter = sv_obj_await(timer, true, SIG_TIMER)
        .into_res()
        .expect("Failed to set wait for timer");
    sv_timer_set(timer, 10000)
        .into_res()
        .expect("Failed to set timer");
    let time = Instant::now();
    sv_obj_awend(waiter, u64::MAX)
        .into_res()
        .expect("Failed to wait for timer");
    log::debug!("Waiting for 10ms, actual passed {:?}", time.elapsed());
    sv_obj_drop(timer).into_res().expect("Failed to drop timer");
}
