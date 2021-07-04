pub mod alloc;
pub mod ctx;
pub(super) mod def;

use self::def::NR_VECTORS;
use crate::cpu::intr::Interrupt;

use ::alloc::sync::Arc;
use spin::Mutex;

const VEC_INTR_INIT: Mutex<Option<Arc<Interrupt>>> = Mutex::new(None);
#[thread_local]
pub static VEC_INTR: [Mutex<Option<Arc<Interrupt>>>; NR_VECTORS] = [VEC_INTR_INIT; NR_VECTORS];

pub struct ArchReg {
      vec: u16,
      cpu: usize,
}
