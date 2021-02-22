use canary::Canary;
use lazy_static::lazy_static;
use paging::{Entry, LAddr};

use core::ops::Range;
use core::ptr::NonNull;
use spin::Mutex;

pub struct Space {
      canary: Canary<Space>,
      root_table: Mutex<NonNull<[Entry]>>,
}

impl Space {
      
}

