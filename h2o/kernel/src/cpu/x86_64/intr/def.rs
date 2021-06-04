//! TODO: Write a macro to define interrupt entries and to define the initial IDT.

macro_rules! define_intr {
      {$vec:expr, $asm_name:ident, $name:ident, $body:block} => {
            extern "C" {
                  pub fn $asm_name();
            }

            #[no_mangle]
            pub extern "C" fn $name() $body
      }
}

// define_intr! {1, rout_dummy, hdl_dummy, {
//       let a = 1;
// }}
