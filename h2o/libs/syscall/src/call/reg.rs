pub trait SerdeReg {
    fn encode(self) -> usize;
    fn decode(val: usize) -> Self;
}

macro_rules! serde_reg_primitives {
    ($($t:ty),*) => {
        $(
            impl SerdeReg for $t {
                #[inline]
                fn encode(self) -> usize {
                      self as _
                }

                #[inline]
                fn decode(reg: usize) -> Self {
                      reg as _
                }
            }
        )*
    }
}

serde_reg_primitives!(u8, u16, u32, usize, i8, i16, i32, isize);
#[cfg(target_pointer_width = "64")]
serde_reg_primitives!(u64, i64);

impl SerdeReg for bool {
    #[inline]
    fn encode(self) -> usize {
        self as usize
    }

    #[inline]
    fn decode(val: usize) -> Self {
        val != 0
    }
}

impl<T> SerdeReg for *const T {
    #[inline]
    fn encode(self) -> usize {
        self as _
    }

    #[inline]
    fn decode(reg: usize) -> Self {
        reg as _
    }
}

impl<T> SerdeReg for *mut T {
    #[inline]
    fn encode(self) -> usize {
        self as _
    }

    #[inline]
    fn decode(reg: usize) -> Self {
        reg as _
    }
}

impl SerdeReg for () {
    #[inline]
    fn encode(self) -> usize {
        0
    }

    #[inline]
    fn decode(_: usize) {}
}
