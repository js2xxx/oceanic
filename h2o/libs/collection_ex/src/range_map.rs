use alloc::collections::{btree_map, BTreeMap};
use core::ops::{Bound, Range, RangeBounds};

use iter_ex::CombineIter;

pub struct RangeIter<'a, K, V> {
    inner: btree_map::Range<'a, K, (Range<K>, V)>,
    range_end: Bound<K>,
}

impl<'a, K: Ord + Copy, V> Iterator for RangeIter<'a, K, V> {
    type Item = (Range<K>, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|(&_, (range, v))| (range.clone(), v))
            .filter(|(r, _v)| match (self.range_end, r.end_bound().cloned()) {
                (Bound::Unbounded, _) => true,
                (_, Bound::Unbounded) => false,
                (Bound::Included(inc), Bound::Included(r_inc)) => r_inc <= inc,
                (Bound::Included(inc), Bound::Excluded(r_exc)) => r_exc <= inc,
                (Bound::Excluded(exc), Bound::Included(r_inc)) => r_inc < exc,
                (Bound::Excluded(exc), Bound::Excluded(r_exc)) => r_exc <= exc,
            })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

pub struct GapIter<'a, K, V> {
    last_end: Option<K>,
    inner: btree_map::Iter<'a, K, (Range<K>, V)>,
}

impl<'a, K: Ord + Copy, V> Iterator for GapIter<'a, K, V> {
    type Item = (Range<K>, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let last_end = self.last_end.get_or_insert(self.inner.next()?.1 .0.end);
        let (_, (range, v)) = self.inner.next()?;

        let ret = *last_end..range.start;
        self.last_end = Some(range.start);

        Some((ret, v))
    }
}

#[derive(Clone, Debug)]
pub struct RangeMap<K, V> {
    inner: BTreeMap<K, (Range<K>, V)>,
}

impl<K: Ord + Copy, V> RangeMap<K, V> {
    pub const fn new() -> Self {
        RangeMap {
            inner: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, range: Range<K>, value: V) -> Result<(), &'static str> {
        if self.range(range.clone()).next().is_some() {
            Err("There are existent item(s) in this range")
        } else {
            self.inner.insert(range.start, (range, value));
            Ok(())
        }
    }

    pub fn remove(&mut self, start: K) -> Option<(Range<K>, V)> {
        self.inner.remove(&start)
    }

    pub fn range_iter(&self) -> RangeIter<'_, K, V> {
        RangeIter {
            inner: self.inner.range(..),
            range_end: Bound::Unbounded,
        }
    }

    pub fn gap_iter(&self) -> GapIter<'_, K, V> {
        GapIter {
            last_end: None,
            inner: self.inner.iter(),
        }
    }

    pub fn range_and_gap_iter(&self) -> iter_ex::Combine<RangeIter<'_, K, V>, GapIter<'_, K, V>> {
        self.range_iter().combine(self.gap_iter())
    }

    pub fn range(&self, range: Range<K>) -> RangeIter<'_, K, V> {
        RangeIter {
            inner: self.inner.range(range.clone()),
            range_end: range.end_bound().cloned(),
        }
    }

    pub fn neighbors(&self, range: Range<K>) -> (Option<Range<K>>, Option<Range<K>>) {
        (
            self.inner
                .range(..=range.start)
                .rev()
                .next()
                .map(|r| r.1 .0.clone())
                .filter(|r| r.end == range.start),
            self.inner
                .range(range.end..)
                .next()
                .map(|r| r.1 .0.clone())
                .filter(|r| r.start == range.end),
        )
    }
}

impl<K: Ord + Copy, V> Default for RangeMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}
