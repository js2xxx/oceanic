macro_rules! rw_simple {
      ($name:ident {$($cons:ident = $bit:expr),*; $($cons0:ident: $tty:ty = $bit0:expr),*}) => {
            #[doc = concat!("The operations of ", stringify!($name), ".")]
            pub mod $name {
                  use core::arch::asm;

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

                  /// # Safety
                  ///
                  /// The caller must ensure the value is valid.
                  #[inline]
                  pub unsafe fn set(bits: u64) {
                        let reg = read();
                        write(reg | bits);
                  }

                  /// # Safety
                  ///
                  /// The caller must ensure the value is valid.
                  #[inline]
                  pub unsafe fn unset(bits: u64) {
                        let reg = read();
                        write(reg & !bits);
                  }

                  $(pub const $cons: u64 = 1 << $bit;)*

                  $(pub const $cons0: $tty = $bit0;)*
            }
      }
}

rw_simple!(cr0 {
    PE = 0,
    MP = 1,
    EM = 2,
    TS = 3,
    ET = 4,
    NE = 5,
    WP = 16,
    AM = 18,
    NW = 29,
    CD = 30,
    PG = 31;
});
rw_simple!(cr2 {; });
rw_simple!(cr3 { PWT = 3, PCD = 4; });
rw_simple!(cr4 {
    VME = 0,
    PVI = 1,
    TSD = 2,
    DE = 3,
    PSE = 4,
    PAE = 5,
    MCE = 6,
    PGE = 7,
    PCE = 8,
    OSFXSR = 9,
    OSXMMEXCPT = 10,
    UMIP = 11,
    LA57 = 12,
    VMXE = 13,
    SMXE = 14,
    FSGSBASE = 16,
    PCIDE = 17,
    OSXSAVE = 18,
    SMEP = 20,
    SMAP = 21,
    PKE = 22,
    CET = 23,
    PKS = 24;
});
rw_simple!(cr8 {; });

rw_simple!(dr0 {; });
rw_simple!(dr1 {; });
rw_simple!(dr2 {; });
rw_simple!(dr3 {; });

rw_simple!(dr6 {
    B0 = 0,
    B1 = 1,
    B2 = 2,
    B3 = 3,
    BD = 13,
    BS = 14,
    BT = 15,
    RTM = 16;
    CLEAR: u64 = 0x0000_0000_FFFF_0FF0
});

rw_simple!(dr7 {
    LE = 8,
    GE = 9,
    RTM = 11,
    GD = 13;
    L: [u64; 4] = [1 << 0, 1 << 2, 1 << 4, 1 << 6],
    G: [u64; 4] = [1 << 1, 1 << 3, 1 << 5, 1 << 7],
    RW: [u64; 4] = [3 << 16, 3 << 20, 3 << 24, 3 << 28],
    LEN: [u64; 4] = [3 << 18, 3 << 22, 3 << 26, 3 << 30]
});

pub const NR_BREAKPOINT: usize = 4;

pub mod rflags {
    use core::arch::asm;

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
    /// The caller must ensure the stack is normal and the operation won't
    /// influence other modules.
    #[inline]
    pub unsafe fn write(val: u64) {
        asm!("push {}; popfq", in(reg) val);
    }

    pub const CF: u64 = 1 << 0;
    pub const _RSVD1: u64 = 1 << 1;
    pub const PF: u64 = 1 << 2;
    pub const AF: u64 = 1 << 4;
    pub const ZF: u64 = 1 << 6;
    pub const SF: u64 = 1 << 7;
    pub const TF: u64 = 1 << 8;
    pub const IF: u64 = 1 << 9;
    pub const DF: u64 = 1 << 10;
    pub const OF: u64 = 1 << 11;
    pub const IOPLL: u64 = 1 << 12;
    pub const IOPLH: u64 = 1 << 13;
    pub const NT: u64 = 1 << 14;
    pub const RF: u64 = 1 << 16;
    pub const VM: u64 = 1 << 17;
    pub const AC: u64 = 1 << 18;
    pub const VIF: u64 = 1 << 19;
    pub const VIP: u64 = 1 << 20;
    pub const ID: u64 = 1 << 21;
    pub const USER_ACCESS: u64 = CF | PF | AF | ZF | SF | TF | DF | OF | NT | AC | ID;
}
