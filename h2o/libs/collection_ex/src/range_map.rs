use alloc::collections::{
    btree_map::{Entry, IntoIter, Iter, IterMut},
    BTreeMap,
};
use core::{
    borrow::Borrow,
    ops::{Add, Bound, Range, RangeBounds, Sub},
};

#[derive(Debug)]
pub struct RangeMap<K, V> {
    inner: BTreeMap<K, (Range<K>, V)>,
    range: Range<K>,
}

impl<K, V> RangeMap<K, V> {
    pub fn new(range: Range<K>) -> Self
    where
        K: Ord + Clone,
    {
        RangeMap {
            inner: BTreeMap::new(),
            range,
        }
    }

    #[inline]
    pub fn range(&self) -> &Range<K> {
        &self.range
    }

    pub fn allocate_with<F, E>(&mut self, size: K, value: F, no_fit: E) -> Result<K, E>
    where
        K: Ord + Sub<Output = K> + Add<Output = K> + Copy,
        F: FnOnce(Range<K>) -> Result<V, E>,
    {
        let mut range = None;

        let mut start = self.range.start;

        for (_, (r, _)) in self.inner.iter() {
            if r.start - start >= size {
                range = Some(start..(start + size));
                break;
            }
            start = r.end;
        }
        if range.is_none() && self.range.end - start >= size {
            range = Some(start..(start + size));
        }

        if let Some(range) = range {
            let start = range.start;
            let value = value(range.clone())?;
            self.inner.entry(start).or_insert((range, value));
            Ok(start)
        } else {
            Err(no_fit)
        }
    }

    pub fn try_insert_with<F, E, R>(
        &mut self,
        range: Range<K>,
        value: F,
        exist: impl Into<E>,
    ) -> Result<R, E>
    where
        K: Ord + Copy,
        F: FnOnce() -> Result<(V, R), E>,
    {
        if self.range.start <= range.start && range.start < range.end && range.end <= self.range.end
        {
            let start = range.start;
            match self.inner.entry(start) {
                Entry::Vacant(ent) => {
                    let (value, ret) = value()?;
                    ent.insert((range, value));
                    Ok(ret)
                }
                Entry::Occupied(_) => Err(exist.into()),
            }
        } else {
            Err(exist.into())
        }
    }

    pub fn get<Q>(&self, start: &Q) -> Option<&V>
    where
        Q: ?Sized + Ord,
        K: Borrow<Q> + Ord,
    {
        self.inner.get(start).map(|(_, value)| value)
    }

    pub fn get_mut<Q>(&mut self, start: &Q) -> Option<&mut V>
    where
        Q: ?Sized + Ord,
        K: Borrow<Q> + Ord,
    {
        self.inner.get_mut(start).map(|(_, value)| value)
    }

    pub fn remove_if<F>(&mut self, start: K, predicate: F) -> Option<(Range<K>, V)>
    where
        K: Ord,
        F: Fn(&V) -> bool,
    {
        match self.inner.entry(start) {
            Entry::Occupied(ent) if predicate(&ent.get().1) => Some(ent.remove()),
            _ => None,
        }
    }

    #[inline]
    pub fn remove(&mut self, start: K) -> Option<(Range<K>, V)>
    where
        K: Ord,
    {
        self.remove_if(start, |_| true)
    }

    #[inline]
    pub fn iter(&self) -> Iter<K, (Range<K>, V)> {
        self.inner.iter()
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<K, (Range<K>, V)> {
        self.inner.iter_mut()
    }

    #[inline]
    pub fn get_contained(&self, key: &K) -> Option<&(Range<K>, V)>
    where
        K: Ord,
    {
        self.inner
            .range(..=key)
            .next_back()
            .map(|(_, value)| value)
            .filter(|(range, _)| key < &range.end)
    }

    #[inline]
    pub fn get_contained_mut(&mut self, key: &K) -> Option<&mut (Range<K>, V)>
    where
        K: Ord,
    {
        self.inner
            .range_mut(..=key)
            .next_back()
            .map(|(_, value)| value)
            .filter(|(range, _)| key < &range.end)
    }

    pub fn get_contained_range<R>(&self, range: R) -> Option<&(Range<K>, V)>
    where
        K: Ord,
        R: RangeBounds<K>,
    {
        let start = match range.start_bound() {
            Bound::Included(start) | Bound::Excluded(start) => start,
            Bound::Unbounded => return None,
        };
        match range.end_bound() {
            Bound::Included(end) => self
                .inner
                .range(..=end)
                .next_back()
                .map(|(_, value)| value)
                .filter(|(range, _)| end < &range.end && range.contains(start)),
            Bound::Excluded(end) => self
                .inner
                .range(..end)
                .next_back()
                .map(|(_, value)| value)
                .filter(|(range, _)| end <= &range.end && range.contains(start)),
            Bound::Unbounded => None,
        }
    }

    #[inline]
    pub fn pop(&mut self) -> Option<(Range<K>, V)>
    where
        K: Ord,
    {
        self.inner.pop_first().map(|(_, value)| value)
    }

    #[inline]
    pub fn first(&self) -> Option<&(Range<K>, V)>
    where
        K: Ord,
    {
        self.inner.first_key_value().map(|(_, value)| value)
    }

    #[inline]
    pub fn last(&self) -> Option<&(Range<K>, V)>
    where
        K: Ord,
    {
        self.inner.last_key_value().map(|(_, value)| value)
    }
}

impl<K, V> IntoIterator for RangeMap<K, V> {
    type Item = (K, (Range<K>, V));

    type IntoIter = IntoIter<K, (Range<K>, V)>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<K: Default, V> Default for RangeMap<K, V> {
    #[inline]
    fn default() -> Self {
        Self {
            inner: Default::default(),
            range: Default::default(),
        }
    }
}
