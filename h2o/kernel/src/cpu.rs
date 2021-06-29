pub mod intr;
pub mod local;

cfg_if::cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
            mod x86_64;
            type ArchCore<'a> = x86_64::Core<'a>;
            type ArchIntr = x86_64::intr::Interrupt;
      }
}

use alloc::sync::Arc;

pub struct Core<'a> {
      arch: ArchCore<'a>,
}

impl<'a> Core<'a> {
      pub fn new(space: &'a Arc<crate::mem::space::Space>) -> Core<'a> {
            Core {
                  arch: ArchCore::new(space),
            }
      }
}
