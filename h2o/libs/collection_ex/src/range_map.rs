use alloc::collections::{
    btree_map::{Entry, IntoIter, Iter, IterMut},
    BTreeMap,
};
use core::{
    borrow::Borrow,
    ops::{Add, Range, Sub},
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

    pub fn allocate_with<F, E>(
        &mut self,
        size: K,
        value: F,
        no_fit: impl Into<E>,
    ) -> Result<(K, &mut V), E>
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
            let (_, value) = self
                .inner
                .entry(start)
                .or_insert((range.clone(), value(range)?));
            Ok((start, value))
        } else {
            Err(no_fit.into())
        }
    }

    pub fn try_insert_with<F, E>(
        &mut self,
        range: Range<K>,
        value: F,
        exist: impl Into<E>,
    ) -> Result<&mut V, E>
    where
        K: Ord + Copy,
        F: FnOnce() -> Result<V, E>,
    {
        if range.start < range.end
            && self.range.contains(&range.start)
            && self.range.contains(&range.end)
        {
            let start = range.start;
            match self.inner.entry(start) {
                Entry::Vacant(ent) => {
                    let (_, value) = ent.insert((range, value()?));
                    Ok(value)
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

    pub fn remove(&mut self, start: &K) -> Option<V>
    where
        K: Ord,
    {
        self.inner.remove(start).map(|(_, value)| value)
    }

    #[inline]
    pub fn iter(&self) -> Iter<K, (Range<K>, V)> {
        self.inner.iter()
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<K, (Range<K>, V)> {
        self.inner.iter_mut()
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
