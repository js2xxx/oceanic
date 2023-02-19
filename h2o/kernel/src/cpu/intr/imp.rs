use alloc::{sync::Arc, vec::Vec};

use crossbeam_queue::ArrayQueue;
use sv_call::{res::Msi, Feature};

use super::{arch::Manager, IntrRes};
use crate::{
    cpu::time::Instant,
    sched::{task::hdl::DefaultFeature, Event, EventData, SIG_GENERIC},
};

const MAX_TIMES: usize = 100;

#[derive(Debug)]
pub struct Interrupt {
    vec: u8,
    cpu: usize,
    last_time: ArrayQueue<Instant>,
    event_data: EventData,
}

impl Event for Interrupt {
    fn event_data(&self) -> &EventData {
        &self.event_data
    }

    fn notify(&self, clear: usize, set: usize) -> usize {
        self.last_time.force_push(Instant::now());

        self.notify_impl(clear, set)
    }
}

impl Interrupt {
    #[inline]
    pub fn new(_: &IntrRes) -> sv_call::Result<Arc<Self>> {
        let cpu = Manager::select_cpu();
        let mut uninit = Arc::try_new_uninit()?;

        let vec = Manager::register(cpu, (handler, uninit.as_ptr() as _))?;
        Arc::get_mut(&mut uninit).unwrap().write(Interrupt {
            vec,
            cpu,
            last_time: ArrayQueue::new(MAX_TIMES),
            event_data: Default::default(),
        });
        // SAFETY: The arc has data written.
        unsafe { Ok(uninit.assume_init()) }
    }

    pub fn new_msi(_: &IntrRes, num: usize) -> sv_call::Result<(Vec<Arc<Self>>, Msi)> {
        let cpu = Manager::select_cpu();
        let msi = Manager::allocate_msi(num.try_into()?, cpu)?;

        let intrs = (msi.vec_start..(msi.vec_start + msi.vec_len))
            .map(|vec| {
                let mut uninit = Arc::try_new_uninit()?;
                Manager::register_handler(cpu, vec, (handler, uninit.as_ptr() as _))?;
                Arc::get_mut(&mut uninit).unwrap().write(Interrupt {
                    vec,
                    cpu,
                    last_time: ArrayQueue::new(MAX_TIMES),
                    event_data: Default::default(),
                });
                // SAFETY: The arc has data written.
                unsafe { Ok(uninit.assume_init()) }
            })
            .collect::<sv_call::Result<Vec<_>>>()?;
        Ok((intrs, msi))
    }

    #[inline]
    pub fn last_time(&self) -> Option<Instant> {
        self.last_time.pop()
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        self.cancel();
        let _ = Manager::deregister(self.vec, self.cpu);
    }
}

unsafe impl DefaultFeature for Interrupt {
    fn default_features() -> Feature {
        Feature::SEND | Feature::WAIT
    }
}

fn handler(arg: *const Interrupt) {
    // SAFETY: The function is only called before the interrupt's destruction, for
    // the calling of `Manager::deregister` in its drop implementation.
    let intr = unsafe { &*arg };
    intr.notify(0, SIG_GENERIC);
}

mod syscall {
    use alloc::sync::Arc;

    use sv_call::{res::Msi, *};

    use super::Interrupt;
    use crate::{
        cpu::{arch::apic::LAPIC_ID, intr::IntrRes},
        sched::SCHED,
        syscall::{Out, UserPtr},
    };

    #[syscall]
    fn intr_new(res: Handle, vec: UserPtr<Out, u8>, apic_id: UserPtr<Out, u32>) -> Result<Handle> {
        res.check_null()?;
        vec.check()?;
        apic_id.check()?;

        SCHED.with_current(|cur| {
            let res = cur.space().handles().get::<IntrRes>(res)?;
            let intr = Interrupt::new(&res)?;
            let a = *LAPIC_ID.read().get(&intr.cpu).unwrap();

            vec.write(intr.vec)?;
            apic_id.write(a)?;

            let event = Arc::downgrade(&intr) as _;
            cur.space().handles().insert(intr, Some(event))
        })
    }

    #[syscall]
    fn intr_msi(
        res: Handle,
        num: usize,
        intr_ptr: UserPtr<Out, Handle>,
        msi_ptr: UserPtr<Out, Msi>,
    ) -> Result {
        res.check_null()?;
        intr_ptr.check_slice(num)?;

        SCHED.with_current(|cur| {
            let res = cur.space().handles().get::<IntrRes>(res)?;

            let (intrs, msi) = Interrupt::new_msi(&res, num)?;

            for (index, intr) in intrs.into_iter().enumerate() {
                let event = Arc::downgrade(&intr) as _;
                let handle = cur.space().handles().insert(intr, Some(event))?;
                let ptr = UserPtr::<Out, _>::new(unsafe { intr_ptr.as_ptr().add(index) });
                ptr.write(handle)?
            }
            msi_ptr.write(msi)
        })
    }

    #[syscall]
    fn intr_query(hdl: Handle, last_time: UserPtr<Out, u128>) -> Result {
        hdl.check_null()?;
        last_time.check()?;

        SCHED.with_current(|cur| {
            let intr = cur.space().handles().get::<Interrupt>(hdl)?;
            let data = intr.last_time().ok_or(ENOENT)?;
            last_time.write(unsafe { data.raw() })
        })
    }
}
