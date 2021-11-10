use alloc::collections::BTreeMap;
use core::ops::RangeInclusive;

use bitop_ex::BitOpEx;
use bitvec::prelude::*;

#[derive(Debug)]
pub struct IdAllocator {
    inner: BTreeMap<u64, BitVec>,
    secondary_bits: u64,
    range: RangeInclusive<u64>,
    next: u64,
}

fn into_idx(sec_bits: u64, val: u64) -> (u64, usize) {
    (
        val >> sec_bits,
        usize::try_from(val & ((1 << sec_bits) - 1)).unwrap(),
    )
}

fn from_idx(sec_bits: u64, prim: u64, sec: usize) -> u64 {
    (prim << sec_bits) | (u64::try_from(sec).unwrap() & ((1 << sec_bits) - 1))
}

impl IdAllocator {
    pub fn new(range: RangeInclusive<u64>) -> Self {
        let init = *range.start();
        let secondary_bits = {
            let bits = if range == (0..=u64::MAX) {
                64
            } else {
                (range.end() - range.start() + 1).log2c()
            };
            (bits >> 1) + (bits & 1)
        };
        IdAllocator {
            inner: BTreeMap::new(),
            secondary_bits,
            range,
            next: init,
        }
    }

    pub fn alloc(&mut self) -> Option<u64> {
        let (prim, sec) = into_idx(self.secondary_bits, self.next);

        let mut insert_bvec = |bvec: &mut BitVec, prim: u64, sec: usize| {
            unsafe { bvec.get_unchecked_mut(sec) }.set(true);
            let id = from_idx(self.secondary_bits, prim, sec);

            self.next = if id == *self.range.end() {
                *self.range.start()
            } else {
                id + 1
            };

            id
        };

        if self.inner.range(prim..=*self.range.end()).next().is_none() {
            let len = 1 << self.secondary_bits;
            let mut bvec = bitvec![0; len];

            let id = (&mut insert_bvec)(&mut bvec, prim, sec);
            let _old = self.inner.insert(prim, bvec);
            debug_assert!(_old.is_none());

            Some(id)
        } else {
            let id = self
                .inner
                .range_mut(prim..=*self.range.end())
                .find_map(|(&prim, bvec)| {
                    bvec.first_zero()
                        .map(|sec| (&mut insert_bvec)(bvec, prim, sec))
                });
            id.or_else(|| {
                self.inner
                    .range_mut(*self.range.start()..prim)
                    .find_map(|(&prim, bvec)| {
                        bvec.first_zero()
                            .map(|sec| (&mut insert_bvec)(bvec, prim, sec))
                    })
            })
        }
    }

    pub fn dealloc(&mut self, id: u64) {
        let (prim, sec) = into_idx(self.secondary_bits, id);
        let bvec = match self.inner.get_mut(&prim) {
            Some(bvec) => bvec,
            None => return,
        };

        let r = match bvec.get_mut(sec) {
            Some(r) => r,
            None => return,
        };

        r.set(false);
    }
}
