#![no_std]

pub use solvent::rxx::*;

#[no_mangle]
extern "C" fn tmain() -> usize {
      let res = unsafe {
            solvent::call::raw::syscall(&solvent::Arguments {
                  fn_num: 0,
                  args: [0; 5],
            })
      };

      solvent::Error::encode(res)
}
