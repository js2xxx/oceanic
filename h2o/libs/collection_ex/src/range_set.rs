use alloc::collections::{btree_map, BTreeMap};
use core::ops::{Bound, Range, RangeBounds};

use iter_ex::CombineIter;

pub struct RangeIter<'a, T> {
    inner: btree_map::Range<'a, T, Range<T>>,
    range_end: Bound<T>,
}

impl<'a, T: Ord + Copy> Iterator for RangeIter<'a, T> {
    type Item = Range<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|i| i.1.clone()).filter(|r| {
            match (self.range_end, r.end_bound().cloned()) {
                (Bound::Unbounded, _) => true,
                (_, Bound::Unbounded) => false,
                (Bound::Included(inc), Bound::Included(r_inc)) => r_inc <= inc,
                (Bound::Included(inc), Bound::Excluded(r_exc)) => r_exc <= inc,
                (Bound::Excluded(exc), Bound::Included(r_inc)) => r_inc < exc,
                (Bound::Excluded(exc), Bound::Excluded(r_exc)) => r_exc <= exc,
            }
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

pub struct GapIter<'a, T> {
    last_end: Option<T>,
    inner: btree_map::Iter<'a, T, Range<T>>,
}

impl<'a, T: Ord + Copy> Iterator for GapIter<'a, T> {
    type Item = Range<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let last_end = self.last_end.get_or_insert(self.inner.next()?.1.end);
        let (_, range) = self.inner.next()?;

        let ret = *last_end..range.start;
        self.last_end = Some(range.start);

        Some(ret)
    }
}

#[derive(Clone, Debug)]
pub struct RangeSet<T> {
    inner: BTreeMap<T, Range<T>>,
}

impl<T: Ord + Copy> RangeSet<T> {
    pub const fn new() -> Self
    where
        T: Ord,
    {
        RangeSet {
            inner: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, range: Range<T>) -> Result<(), &'static str> {
        if self.range(range.clone()).next().is_some() {
            Err("There are existent item(s) in this range")
        } else {
            self.inner.insert(range.start, range);
            Ok(())
        }
    }

    pub fn remove(&mut self, start: T) -> Option<Range<T>> {
        self.inner.remove(&start)
    }

    pub fn range_iter(&self) -> RangeIter<'_, T> {
        RangeIter {
            inner: self.inner.range(..),
            range_end: Bound::Unbounded,
        }
    }

    pub fn gap_iter(&self) -> GapIter<'_, T> {
        GapIter {
            last_end: None,
            inner: self.inner.iter(),
        }
    }

    pub fn range_and_gap_iter(&self) -> iter_ex::Combine<RangeIter<'_, T>, GapIter<'_, T>> {
        self.range_iter().combine(self.gap_iter())
    }

    pub fn range(&self, range: Range<T>) -> RangeIter<'_, T> {
        RangeIter {
            inner: self.inner.range(range.clone()),
            range_end: range.end_bound().cloned(),
        }
    }

    pub fn neighbors(&self, range: Range<T>) -> (Option<Range<T>>, Option<Range<T>>) {
        (
            self.inner
                .range(..=range.start)
                .rev()
                .next()
                .map(|r| r.1.clone())
                .filter(|r| r.end == range.start),
            self.inner
                .range(range.end..)
                .next()
                .map(|r| r.1.clone())
                .filter(|r| r.start == range.end),
        )
    }
}

impl<T: Ord + Copy> Default for RangeSet<T> {
    fn default() -> Self {
        Self::new()
    }
}
