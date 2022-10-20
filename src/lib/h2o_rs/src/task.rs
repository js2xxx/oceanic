#[cfg(feature = "stub")]
use core::num::NonZeroUsize;
use core::{
    convert::TryInto,
    mem,
    ptr::{null, null_mut, NonNull},
    time::Duration,
};

pub use sv_call::task::{ctx::Gpr, *};
use sv_call::{ipc::SIG_READ, Error, Handle, SV_SUSPENDTOKEN, SV_TASK};

use crate::{error::Result, ipc::Channel, mem::Space, obj::Object};

#[repr(transparent)]
#[derive(Debug)]
pub struct Task(sv_call::Handle);
crate::impl_obj!(Task, SV_TASK);
crate::impl_obj!(@DROP, Task);

impl Task {
    pub fn try_new(
        name: Option<&str>,
        space: Option<Space>,
        init_chan: Option<Channel>,
    ) -> Result<(Self, SuspendToken)> {
        let name = name.map(|name| name.as_bytes());
        let mut st = Handle::NULL;
        let handle = unsafe {
            sv_call::sv_task_new(
                name.map_or(null(), |name| name.as_ptr()),
                name.map_or(0, |name| name.len()),
                space.map_or(Handle::NULL, Space::into_raw),
                init_chan.map_or(Handle::NULL, Channel::into_raw),
                &mut st,
            )
            .into_res()?
        };
        // SAFETY: The handles are freshly allocated.
        Ok(unsafe { (Self::from_raw(handle), SuspendToken::from_raw(st)) })
    }

    pub fn new(
        name: Option<&str>,
        space: Option<Space>,
        init_chan: Option<Channel>,
    ) -> (Self, SuspendToken) {
        Self::try_new(name, space, init_chan).expect("Failed to create a task")
    }

    pub fn exec(
        name: Option<&str>,
        space: Option<Space>,
        entry: NonNull<u8>,
        stack: NonNull<u8>,
        init_chan: Option<Channel>,
        arg2: u64,
    ) -> Result<Self> {
        let name = name.map(|name| name.as_bytes());
        let ci = ExecInfo {
            name: name.map_or(null(), |name| name.as_ptr()),
            name_len: name.map_or(0, |name| name.len()),
            space: space.map_or(Handle::NULL, Space::into_raw),
            entry: entry.as_ptr(),
            stack: stack.as_ptr(),
            init_chan: init_chan.map_or(Handle::NULL, Channel::into_raw),
            arg: arg2,
        };
        let handle = unsafe { sv_call::sv_task_exec(&ci).into_res()? };
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Self::from_raw(handle) })
    }

    pub fn try_join(self) -> core::result::Result<usize, (Error, Self)> {
        // SAFETY: We don't move the ownership of the handle...
        let mut ret = Default::default();
        let res = unsafe { sv_call::sv_task_join(unsafe { self.raw() }, &mut ret).into_res() };
        match res {
            Ok(()) => {
                // ...unless the operation is successful.
                mem::forget(self);
                Ok(ret)
            }
            Err(err) => Err((err, self)),
        }
    }

    pub fn join(self) -> Result<usize> {
        self.try_wait(Duration::MAX, true, false, SIG_READ)?;
        self.try_join().map_err(|(err, _)| err)
    }

    pub fn kill(&self) -> Result {
        unsafe {
            // SAFETY: We don't move the ownership of the handle.
            sv_call::sv_task_ctl(unsafe { self.raw() }, TASK_CTL_KILL, null_mut()).into_res()
        }
    }

    pub fn suspend(&self) -> Result<SuspendToken> {
        let mut st = Handle::NULL;
        unsafe {
            // SAFETY: We don't move the ownership of the handle.
            sv_call::sv_task_ctl(unsafe { self.raw() }, TASK_CTL_SUSPEND, &mut st).into_res()?
        };
        // SAFETY: The handles are freshly allocated.
        Ok(unsafe { SuspendToken::from_raw(st) })
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub struct SuspendToken(sv_call::Handle);
crate::impl_obj!(SuspendToken, SV_SUSPENDTOKEN);
crate::impl_obj!(@DROP, SuspendToken);

impl SuspendToken {
    pub fn read_memory_into(&self, addr: usize, buffer: &mut [u8]) -> Result {
        unsafe {
            sv_call::sv_task_debug(
                // SAFETY: We don't move the ownership of the handle.
                unsafe { self.raw() },
                TASK_DBG_READ_MEM,
                addr,
                buffer.as_mut_ptr(),
                buffer.len(),
            )
            .into_res()
        }
    }

    /// # Safety
    ///
    /// The caller must ensure the memory safety.
    pub unsafe fn write_memory(&self, addr: usize, buffer: &[u8]) -> Result {
        sv_call::sv_task_debug(
            // SAFETY: We don't move the ownership of the handle.
            unsafe { self.raw() },
            TASK_DBG_WRITE_MEM,
            addr,
            buffer.as_ptr() as *mut u8,
            buffer.len(),
        )
        .into_res()
    }

    pub fn read_gpr_into(&self, gpr: &mut Gpr) -> Result {
        unsafe {
            sv_call::sv_task_debug(
                // SAFETY: We don't move the ownership of the handle.
                unsafe { self.raw() },
                TASK_DBG_READ_REG,
                TASK_DBGADDR_GPR,
                gpr as *mut _ as *mut _,
                mem::size_of::<Gpr>(),
            )
            .into_res()
        }
    }

    pub fn read_gpr(&self) -> Result<Gpr> {
        let mut ret = Default::default();
        self.read_gpr_into(&mut ret)?;
        Ok(ret)
    }

    pub fn write_gpr(&self, gpr: &Gpr) -> Result {
        unsafe {
            sv_call::sv_task_debug(
                // SAFETY: We don't move the ownership of the handle.
                unsafe { self.raw() },
                TASK_DBG_WRITE_REG,
                TASK_DBGADDR_GPR,
                gpr as *const _ as *mut u8,
                mem::size_of::<Gpr>(),
            )
            .into_res()
        }
    }

    #[inline]
    pub fn wake(self) {
        let _ = self;
    }
}

/// # Safety
///
/// This function doesn't clean up the current self-maintained context, and the
/// caller must ensure it is destroyed before calling this function.
pub unsafe fn exit(retval: usize) -> ! {
    let _ = sv_call::sv_task_exit(retval);
    unreachable!("The task failed to exit");
}

pub fn sleep(duration: Duration) -> Result {
    let millis = duration.as_millis().try_into()?;
    unsafe { sv_call::sv_task_sleep(millis).into_res() }
}

#[cfg(feature = "stub")]
#[inline]
pub fn cpu_num() -> NonZeroUsize {
    let res = unsafe { sv_call::sv_cpu_num().into_res() };
    NonZeroUsize::new(res.unwrap() as usize).unwrap()
}
