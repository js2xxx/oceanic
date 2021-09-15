#![no_std]
#![feature(const_type_id)]
#![feature(core_intrinsics)]

use core::any::{type_name, TypeId};
use core::intrinsics::unlikely;
use core::marker::PhantomData;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Canary<T> {
      id: TypeId,
      _marker: PhantomData<T>,
}

impl<T: 'static> Canary<T> {
      pub const fn new() -> Self {
            Canary {
                  id: TypeId::of::<T>(),
                  _marker: PhantomData,
            }
      }

      pub fn check(&self) -> bool {
            self.id == TypeId::of::<T>()
      }

      #[track_caller]
      pub fn assert(&self) {
            if unlikely(!self.check()) {
                  panic!(
                        "Canary of type {} ({:?}) check failed, invalid value = {:?}, from function {}",
                        type_name::<T>(),
                        TypeId::of::<T>(),
                        self.id,
                        core::panic::Location::caller()
                  );
            }
      }
}

impl<T: 'static> Default for Canary<T> {
      fn default() -> Self {
            Canary::new()
      }
}

impl<T: 'static> core::fmt::Debug for Canary<T> {
      fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            if self.check() {
                  write!(f, "{}", core::any::type_name::<T>())
            } else {
                  write!(f, "<Invalid type>")
            }
      }
}
