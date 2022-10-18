use alloc::vec::Vec;
use core::error::Error;

use solvent::prelude::Channel;
use solvent_fs::fs;

use crate::{
    env,
    thread::{self, Thread},
};

pub(crate) static mut ARGS: Vec<u8> = Vec::new();

pub trait Termination {
    fn report(self) -> usize;
}

impl Termination for usize {
    fn report(self) -> usize {
        self
    }
}

impl Termination for () {
    fn report(self) -> usize {
        0
    }
}

impl Termination for ! {
    fn report(self) -> usize {
        self
    }
}

impl<E: Error> Termination for Result<(), E> {
    fn report(self) -> usize {
        match self {
            Ok(()) => ().report(),
            Err(e) => Err::<!, _>(e).report(),
        }
    }
}

impl<E: Error> Termination for Result<!, E> {
    fn report(self) -> usize {
        match self {
            Ok(t) => t,
            Err(e) => log::error!("main function ended with error: {e}"),
        }
        1
    }
}

#[doc(hidden)]
pub fn lang_start<R: Termination>(channel: Channel, main: fn() -> R) -> R {
    #[link(name = "ldso")]
    extern "C" {
        fn __libc_start_init();
        fn __libc_exit_fini();
    }

    let args = svrt::init_rt(&channel).expect("Failed to initialize runtime");

    unsafe {
        __libc_start_init();
        ARGS = args;

        // TODO: Remove this in the future.
        dbglog::init(log::Level::Debug);

        thread::current::set(Thread::new(None));
    }

    let ret = {
        let (_, cwd) = env::vars().find(|(key, _)| key == "CWD").unzip();
        let (_, paths) = env::vars().find(|(key, _)| key == "LFS").unzip();
        svrt::with_startup_args(|sa| unsafe {
            if let Some(paths) = paths {
                fs::init_rt(&mut sa.handles, paths.split(','), cwd.as_deref())
            } else {
                fs::init_rt(
                    &mut sa.handles,
                    ([] as [&str; 0]).into_iter(),
                    cwd.as_deref(),
                )
            }
        });

        let ret = main();

        unsafe { fs::fini_rt() };

        ret
    };
    unsafe {
        ARGS = Vec::new();
        __libc_exit_fini();
    }

    ret
}

#[macro_export]
macro_rules! entry {
    ($main:ident) => {
        #[no_mangle]
        extern "C" fn _start(init_chan: solvent::prelude::Handle) {
            $crate::rt::lang_start(
                unsafe { solvent::prelude::Object::from_raw(init_chan) },
                $main,
            );
            unsafe { solvent::task::exit(0) };
        }
    };
}
