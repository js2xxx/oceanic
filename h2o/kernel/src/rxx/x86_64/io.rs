//! I/O operations for the kernel

/// Execute OUT opcode (8 bits)
///
/// # Safety
///
/// 1. The I/O permission must be satisfied.
/// 2. The `port` and `val` must be invalid; more precisely, the I/O port must be present
///    and the value must satisfy the demands of I/O devices.
pub unsafe fn out8(port: u16, val: u8) {
      asm!("out dx, al", in("dx") port, in("al") val);
}

/// Execute OUT opcode (16 bits)
///
/// # Safety
///
/// See [`out8`] for more.
pub unsafe fn out16(port: u16, val: u16) {
      asm!("out dx, ax", in("dx") port, in("ax") val);
}

/// Execute OUT opcode (32 bits)
///
/// # Safety
///
/// See [`out8`] for more.
pub unsafe fn out32(port: u16, val: u32) {
      asm!("out dx, eax", in("dx") port, in("eax") val);
}

/// Execute IN opcode (8 bits)
///
/// # Safety
///
/// The `port` and `val` must be invalid; more precisely, the I/O port must be present
/// and the value must satisfy the demands of I/O devices.
pub unsafe fn in8(port: u16) -> u8 {
      let ret: u8;
      asm!("in al, dx", out("al") ret, in("dx") port);
      ret
}

/// Execute IN opcode (16 bits)
///
/// # Safety
///
/// See [`in8`] for more.
pub unsafe fn in16(port: u16) -> u16 {
      let ret: u16;
      asm!("in ax, dx", out("ax") ret, in("dx") port);
      ret
}

/// Execute IN opcode (32 bits)
///
/// # Safety
///
/// See [`in8`] for more.
pub unsafe fn in32(port: u16) -> u32 {
      let ret: u32;
      asm!("in eax, dx", out("eax") ret, in("dx") port);
      ret
}
