cfg_if::cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
            pub use crate::x86_64::io::*;
      }
}

pub trait Io {
      type Val: num_traits::Num + Copy;
      type Off: num_traits::Zero;

      /// Execute IN opcode
      ///
      /// # Safety
      ///
      /// The `val` ans `offset` must be valid; more precisely, the port adding the offset must be
      /// present and the value must satisfy the demands of I/O devices.
      unsafe fn read_offset(&self, offset: Self::Off) -> Self::Val;

      /// Execute OUT opcode
      ///
      /// # Safety
      ///
      /// 1. The I/O permission must be satisfied.
      /// 2. The `val` ans `offset` must be valid; more precisely, the port adding the offset must 
      /// be present and the value must satisfy the demands of I/O devices.
      unsafe fn write_offset(&mut self, offset: Self::Off, value: Self::Val);

      /// Execute IN opcode
      ///
      /// # Safety
      ///
      /// The `val` must be valid; more precisely, the value must satisfy the demands of I/O
      /// devices.
      unsafe fn read(&self) -> Self::Val {
            self.read_offset(<Self::Off as num_traits::Zero>::zero())
      }

      /// Execute OUT opcode
      ///
      /// # Safety
      ///
      /// 1. The I/O permission must be satisfied.
      /// 2. The `value` must be valid; more precisely, the value must satisfy the demands of
      /// I/O devices.
      unsafe fn write(&mut self, value: Self::Val) {
            self.write_offset(<Self::Off as num_traits::Zero>::zero(), value)
      }
}
