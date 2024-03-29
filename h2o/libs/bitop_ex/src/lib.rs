#![no_std]
#![feature(core_intrinsics)]

use core::{intrinsics as ci, num::Wrapping, ops::*};

use num_traits::{Num, NumCast};

pub trait BitOpEx:
    Sized
    + Num
    + NumCast
    + Copy
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
    + BitXor<Output = Self>
    + Shl<Output = Self>
    + Shr<Output = Self>
    + Not<Output = Self>
where
    Wrapping<Self>: Add<Output = Wrapping<Self>>
        + Sub<Output = Wrapping<Self>>
        + Mul<Output = Wrapping<Self>>
        + Div<Output = Wrapping<Self>>
        + Rem<Output = Wrapping<Self>>,
{
    const BIT_SIZE: usize = core::mem::size_of::<Self>() * 8;

    #[inline]
    #[must_use]
    fn round_up_bit(&self, bit: Self) -> Self {
        let val = Wrapping(Self::one() << bit);
        let this = Wrapping(*self);
        let one = Wrapping(Self::one());
        (Wrapping((this - one).0 | (val - one).0) + one).0
    }

    #[inline]
    #[must_use]
    fn round_down_bit(&self, bit: Self) -> Self {
        let val = Self::one() << bit;
        *self & !(Wrapping(val) - Wrapping(Self::one())).0
    }

    #[inline]
    #[must_use]
    fn div_ceil_bit(&self, bit: Self) -> Self {
        self.round_up_bit(bit) >> bit
    }

    #[inline]
    #[must_use]
    fn lsb(&self) -> Self {
        ci::cttz(*self)
    }

    #[inline]
    #[must_use]
    fn msb(&self) -> Self {
        (Wrapping(Self::from(Self::BIT_SIZE).unwrap()) - Wrapping(ci::ctlz(*self) + Self::one())).0
    }

    #[inline]
    #[must_use]
    fn log2f(&self) -> Self {
        self.msb()
    }

    #[inline]
    #[must_use]
    fn log2c(&self) -> Self {
        let msb = self.msb();
        msb + Self::from((msb != self.lsb()) as usize).unwrap()
    }

    #[inline]
    fn contains_bit(&self, bit: Self) -> bool {
        (*self & bit) != Self::zero()
    }
}

impl<T> BitOpEx for T
where
    T: Sized
        + Num
        + NumCast
        + Copy
        + BitAnd<Output = Self>
        + BitOr<Output = Self>
        + BitXor<Output = Self>
        + Shl<Output = Self>
        + Shr<Output = Self>
        + Not<Output = Self>,
    Wrapping<Self>: Add<Output = Wrapping<Self>>
        + Sub<Output = Wrapping<Self>>
        + Mul<Output = Wrapping<Self>>
        + Div<Output = Wrapping<Self>>
        + Rem<Output = Wrapping<Self>>,
{
}
