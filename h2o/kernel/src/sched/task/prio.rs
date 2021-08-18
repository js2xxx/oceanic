use core::ops::Range;

pub const PRIO_RANGE: Range<Priority> = Priority(0)..Priority(40);
pub const IDLE: Priority = Priority(0);
pub const DEFAULT: Priority = Priority(20);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(u16);
