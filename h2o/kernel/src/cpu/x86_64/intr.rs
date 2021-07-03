pub mod alloc;
pub mod ctx;
pub(super) mod def;

pub struct Interrupt {
      vec: u16,
      cpu: usize,
}

impl Interrupt {}
