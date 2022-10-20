mod builder;

use core::{mem, ops::Deref, ptr::NonNull};

use solvent::task::{SuspendToken, Task};
use solvent_rpc::SerdePacket;

pub use self::builder::{Builder, Error as BuildError};

pub enum Error {
    Exited(usize),
    Started,
    Start(solvent::error::Error),
    Join(solvent::error::Error),
    TryJoin(solvent::error::Error),
    Suspend(solvent::error::Error),
    Kill(solvent::error::Error),
    Wait(solvent::error::Error),
}

#[derive(SerdePacket)]
pub struct InitProcess {
    task: Task,
    entry: NonNull<u8>,
    stack: NonNull<u8>,
    vdso_base: NonNull<u8>,
    suspend_token: SuspendToken,
}

unsafe impl Send for InitProcess {}

impl Deref for InitProcess {
    type Target = SuspendToken;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.suspend_token
    }
}

impl InitProcess {
    pub fn start(self) -> Result<Process, Error> {
        let InitProcess {
            task,
            entry,
            stack,
            vdso_base,
            suspend_token,
        } = self;
        let mut gpr = suspend_token.read_gpr().map_err(Error::Start)?;
        gpr.rip = entry.as_ptr() as _;
        gpr.rsp = stack.as_ptr() as _;
        gpr.rsi = vdso_base.as_ptr() as _;
        suspend_token.write_gpr(&gpr).map_err(Error::Start)?;
        Ok(Process::new(task))
    }
}

enum ProcessState {
    Started(Task),
    Exited(usize),
}

impl ProcessState {
    fn kill(&mut self) -> Result<(), Error> {
        match self {
            ProcessState::Started(task) => task.kill().map_err(Error::Kill),
            ProcessState::Exited(status) => Err(Error::Exited(*status)),
        }
    }

    fn join(&mut self) -> Result<usize, Error> {
        let status = match mem::replace(self, ProcessState::Exited(0)) {
            ProcessState::Started(task) => task.join().map_err(Error::Join)?,
            ProcessState::Exited(status) => status,
        };
        Ok(status)
    }

    fn try_join(&mut self) -> Result<usize, Error> {
        let status = match mem::replace(self, ProcessState::Exited(0)) {
            ProcessState::Started(task) => match task.try_join() {
                Ok(status) => status,
                Err((err, task)) => {
                    *self = ProcessState::Started(task);
                    return Err(Error::TryJoin(err));
                }
            },
            ProcessState::Exited(status) => status,
        };
        *self = ProcessState::Exited(status);
        Ok(status)
    }
}

pub struct Process(ProcessState);

unsafe impl Send for Process {}
unsafe impl Sync for Process {}

impl Process {
    fn new(task: Task) -> Self {
        Process(ProcessState::Started(task))
    }

    #[inline]
    pub fn builder() -> Builder {
        Default::default()
    }

    pub fn suspend(&self) -> Result<SuspendToken, Error> {
        match self.0 {
            ProcessState::Started(ref task) => Ok(task.suspend().map_err(Error::Suspend)?),
            ProcessState::Exited(status) => Err(Error::Exited(status)),
        }
    }

    #[inline]
    pub fn kill(&mut self) -> Result<(), Error> {
        self.0.kill()
    }

    #[inline]
    pub fn join(&mut self) -> Result<usize, Error> {
        self.0.join()
    }

    #[inline]
    pub fn try_join(&mut self) -> Result<usize, Error> {
        self.0.try_join()
    }
}

#[cfg(feature = "runtime")]
mod runtime {
    use core::{
        future::Future,
        mem,
        pin::Pin,
        task::{Context, Poll},
    };

    use futures::pin_mut;
    use solvent::prelude::SIG_READ;
    use solvent_async::ipc::AsyncObject;

    use super::{Error, Process};
    use crate::process::ProcessState;

    // TODO: Replace with proactor API.
    impl Future for Process {
        type Output = Result<usize, Error>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let fut = async move {
                let status = match mem::replace(&mut self.0, ProcessState::Exited(0)) {
                    ProcessState::Started(task) => {
                        task.try_wait_with(&solvent_async::dispatch(), true, SIG_READ)
                            .await
                            .map_err(Error::Wait)?;
                        task.join().map_err(Error::Join)?
                    }
                    ProcessState::Exited(status) => status,
                };
                self.0 = ProcessState::Exited(status);
                Ok(status)
            };
            pin_mut!(fut);
            fut.poll(cx)
        }
    }
}
#[cfg(feature = "runtime")]
pub use runtime::*;
