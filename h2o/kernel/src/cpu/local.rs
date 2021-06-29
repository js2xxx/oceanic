//! TODO: Implement this module after ACPI static table initialization.

use alloc::collections::BTreeMap;
use alloc::collections::LinkedList;
use core::alloc::Layout;
use core::marker::PhantomData;
use spin::Mutex;

static ALLOC_ID: Mutex<u32> = Mutex::new(0);
static mut ALLOC_MAP: LinkedList<DynAlloc> = LinkedList::new();

struct DynAlloc {
      obj_id: u32,
      obj_layout: Layout,
      ptr_map: BTreeMap<u32, *mut u8>,
}

fn alloc_object(obj_id: u32, obj_layout: Layout, cpuid: u32) -> *mut u8 {
      // let alloc = match ALLOC_ID
      todo!()
}

pub struct DynObject<T> {
      base_ptr: *mut u8,
      _marker: PhantomData<*mut T>,
}

impl<T> DynObject<T> {
      pub fn new(val: T) -> DynObject<T> {
            todo!()
      }

      pub fn as_ref(&self, cpuid: u32) -> &T {
            todo!()
      }

      pub fn as_mut(&mut self, cpuid: u32) -> &mut T {
            todo!()
      }

      pub fn as_ptr(&self, cpuid: u32) -> *const T {
            todo!()
      }

      pub fn as_mut_ptr(&mut self, cpuid: u32) -> *mut T {
            todo!()
      }
}
