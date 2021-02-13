macro_rules! rw_simple {
      ($name:ident) => {
            /// The operations of $name.
            pub mod $name {
                  /// # Safety
                  ///
                  /// The caller must use the value under a certain limit.
                  #[inline]
                  pub unsafe fn read() -> u64 {
                        let mut ret;
                        asm!(concat!("mov {}, ", stringify!($name)), out(reg) ret);
                        ret
                  }

                  /// # Safety
                  ///
                  /// The caller must ensure the value is valid.
                  #[inline]
                  pub unsafe fn write(val: u64) {
                        asm!(concat!("mov ", concat!(stringify!($name), ", {}")), in(reg) val);
                  }
            }
      }
}

rw_simple!(cr0);
rw_simple!(cr2);
rw_simple!(cr3);
rw_simple!(cr4);
rw_simple!(cr8);

pub mod rflags {
      /// Read RFLAGS of the current CPU.
      ///
      /// # Safety
      ///
      /// The caller must ensure the stack is normal.
      #[inline]
      pub unsafe fn read() -> u64 {
            let mut ret;
            asm!("pushfq; pop {}", out(reg) ret);
            ret
      }

      /// Write RFLAGS of the current CPU.
      ///
      /// # Safety
      ///
      /// The caller must ensure the stack is normal and the operation won't influence other
      /// modules.
      #[inline]
      pub unsafe fn write(val: u64) {
            asm!("push {}; popfq", in(reg) val);
      }
}
