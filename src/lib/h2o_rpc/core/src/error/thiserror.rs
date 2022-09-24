//! Copied from `thiserror` crate.

pub mod __private {
    use core::{error::Error, panic::UnwindSafe};

    pub trait AsDynError<'a>: Sealed {
        fn as_dyn_error(&self) -> &(dyn Error + 'a);
    }

    impl<'a, T: Error + 'a> AsDynError<'a> for T {
        #[inline]
        fn as_dyn_error(&self) -> &(dyn Error + 'a) {
            self
        }
    }

    impl<'a> AsDynError<'a> for dyn Error + 'a {
        #[inline]
        fn as_dyn_error(&self) -> &(dyn Error + 'a) {
            self
        }
    }

    impl<'a> AsDynError<'a> for dyn Error + Send + 'a {
        #[inline]
        fn as_dyn_error(&self) -> &(dyn Error + 'a) {
            self
        }
    }

    impl<'a> AsDynError<'a> for dyn Error + Send + Sync + 'a {
        #[inline]
        fn as_dyn_error(&self) -> &(dyn Error + 'a) {
            self
        }
    }

    impl<'a> AsDynError<'a> for dyn Error + Send + Sync + UnwindSafe + 'a {
        #[inline]
        fn as_dyn_error(&self) -> &(dyn Error + 'a) {
            self
        }
    }

    pub trait Sealed {}
    impl<'a, T: Error + 'a> Sealed for T {}
    impl<'a> Sealed for dyn Error + 'a {}
    impl<'a> Sealed for dyn Error + Send + 'a {}
    impl<'a> Sealed for dyn Error + Send + Sync + 'a {}
    impl<'a> Sealed for dyn Error + Send + Sync + UnwindSafe + 'a {}

    use core::fmt::Display;

    pub trait DisplayAsDisplay {
        fn as_display(&self) -> Self;
    }

    impl<T: Display> DisplayAsDisplay for &T {
        fn as_display(&self) -> Self {
            self
        }
    }

    pub trait PathAsDisplay {
        fn as_display(&self) -> Self;
    }
}
