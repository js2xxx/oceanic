mod inner;

use core::{
    borrow::Borrow,
    fmt,
    hash::{BuildHasher, Hash},
    hint, mem,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicUsize, Ordering::*},
};

use spin::{RwLock, RwLockReadGuard, RwLockWriteGuard};

const GROW_FACTOR: usize = 2;
const LOAD_FACTOR_N: usize = 70;
const LOAD_FACTOR_D: usize = 100;
const MIN_CAPACITY: usize = 8;

pub struct ReadGuard<'a, K, V, S> {
    _buckets: RwLockReadGuard<'a, inner::Buckets<K, V, S>>,
    inner: RwLockReadGuard<'a, inner::Entry<(K, V)>>,
}

impl<'a, K, V, S> ReadGuard<'a, K, V, S> {
    pub fn key(&self) -> &K {
        &self.inner.get().unwrap().0
    }
}

impl<'a, K, V, S> Deref for ReadGuard<'a, K, V, S> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.inner.get().unwrap().1
    }
}

pub struct WriteGuard<'a, K, V, S> {
    buckets: RwLockReadGuard<'a, inner::Buckets<K, V, S>>,
    inner: RwLockWriteGuard<'a, inner::Entry<(K, V)>>,
}

impl<'a, K, V, S> WriteGuard<'a, K, V, S> {
    pub fn key(&self) -> &K {
        &self.inner.get().unwrap().0
    }

    pub fn downgrade(self) -> ReadGuard<'a, K, V, S> {
        ReadGuard {
            _buckets: self.buckets,
            inner: self.inner.downgrade(),
        }
    }
}

impl<'a, K, V, S> Deref for WriteGuard<'a, K, V, S> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.inner.get().unwrap().1
    }
}

impl<'a, K, V, S> DerefMut for WriteGuard<'a, K, V, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner.get_mut().unwrap().1
    }
}

pub struct CHashMap<K, V, S> {
    inner: RwLock<inner::Buckets<K, V, S>>,
    len: AtomicUsize,
}

unsafe impl<K: Send, V: Send, S: Send> Send for CHashMap<K, V, S> {}
unsafe impl<K: Sync + Send, V: Sync + Send, S> Sync for CHashMap<K, V, S> {}

impl<K, V, S: Default> Default for CHashMap<K, V, S> {
    #[inline]
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}

impl<K, V, S> CHashMap<K, V, S> {
    pub fn with_hasher(hasher: S) -> Self {
        CHashMap {
            inner: RwLock::new(inner::Buckets::with_capacity(hasher, MIN_CAPACITY)),
            len: AtomicUsize::new(0),
        }
    }

    pub fn len(&self) -> usize {
        self.len.load(SeqCst)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<K, V, S: Clone> CHashMap<K, V, S> {
    pub fn take(&self) -> CHashMap<K, V, S> {
        let mut buckets = self.inner.write();

        let mut ret = Self::with_hasher(buckets.hasher().clone());
        *ret.len.get_mut() = self.len.swap(0, SeqCst);
        mem::swap(ret.inner.get_mut(), &mut *buckets);

        ret
    }
}

impl<K, V, S: BuildHasher + Default> CHashMap<K, V, S> {
    fn grow(&self, old_len: usize)
    where
        K: Hash,
    {
        let len = old_len * GROW_FACTOR;
        let mut buckets = self.inner.write();
        if buckets.len() < len {
            let new = inner::Buckets::with_capacity(S::default(), len);
            let old = mem::replace(&mut *buckets, new);
            buckets.move_from(old);
        }
    }

    fn shrink(&self, new_len: usize)
    where
        K: Hash,
    {
        let mut buckets = self.inner.write();
        if buckets.len() > new_len {
            let new = inner::Buckets::with_capacity(S::default(), new_len.max(MIN_CAPACITY));
            let old = mem::replace(&mut *buckets, new);
            buckets.move_from(old);
        }
    }

    pub fn get<'a, Q>(&'a self, key: &Q) -> Option<ReadGuard<'a, K, V, S>>
    where
        Q: Hash + PartialEq,
        K: Borrow<Q>,
    {
        let buckets = self.inner.read();
        let entry = unsafe { &*(&buckets as *const RwLockReadGuard<inner::Buckets<K, V, S>>) }
            .find_read(key, |entry| entry.contains_key(key));
        entry.map(|entry| ReadGuard {
            _buckets: buckets,
            inner: entry,
        })
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        Q: Hash + PartialEq,
        K: Borrow<Q>,
    {
        self.get(key).is_some()
    }

    pub fn get_mut<'a, Q>(&'a self, key: &Q) -> Option<WriteGuard<'a, K, V, S>>
    where
        Q: Hash + PartialEq,
        K: Borrow<Q>,
    {
        let buckets = self.inner.read();
        let entry = unsafe { &*(&buckets as *const RwLockReadGuard<inner::Buckets<K, V, S>>) }
            .find_write(key, |entry| entry.contains_key(key));
        entry.map(|entry| WriteGuard {
            buckets,
            inner: entry,
        })
    }

    pub fn insert(&self, key: K, value: V) -> Option<(K, V)>
    where
        K: Hash + PartialEq,
    {
        loop {
            let buckets = self.inner.read();
            let old = match buckets.entry(&key) {
                Some(mut entry) => mem::replace(&mut *entry, inner::Entry::Data((key, value))),
                None => {
                    hint::spin_loop();
                    continue;
                }
            };
            if old.is_free() {
                let len = self.len.fetch_add(1, SeqCst) + 1;
                if len * LOAD_FACTOR_D >= buckets.len() * LOAD_FACTOR_N {
                    drop(buckets);
                    self.grow(len);
                }
            }
            break old.into();
        }
    }

    pub fn get_or_insert(&self, key: K, value: V) -> WriteGuard<'_, K, V, S>
    where
        K: Hash + PartialEq + Clone,
    {
        let buckets = self.inner.read();
        let mut entry = loop {
            match unsafe { &*(&buckets as *const RwLockReadGuard<inner::Buckets<K, V, S>>) }
                .entry(&key)
            {
                Some(entry) => break entry,
                None => hint::spin_loop(),
            }
        };

        if entry.is_free() {
            *entry = inner::Entry::Data((key.clone(), value));
            let len = self.len.fetch_add(1, SeqCst) + 1;
            if len * LOAD_FACTOR_D >= buckets.len() * LOAD_FACTOR_N {
                drop(entry);
                drop(buckets);
                self.grow(len);

                return self.get_mut(&key).unwrap();
            }
        }

        WriteGuard {
            buckets,
            inner: entry,
        }
    }

    /// # Safety
    ///
    /// `guard` must come from [`Self::get_mut`] method.
    pub unsafe fn remove_from(&self, mut guard: WriteGuard<'_, K, V, S>) -> (K, V)
    where
        K: Hash,
    {
        let ret = mem::replace(&mut *guard.inner, inner::Entry::Removed);
        let len = self.len.fetch_sub(1, SeqCst) - 1;
        if len * GROW_FACTOR * LOAD_FACTOR_D < guard.buckets.len() * LOAD_FACTOR_N {
            drop(guard);
            self.shrink(len);
        }
        Option::<(K, V)>::from(ret).unwrap()
    }

    pub fn remove_entry_if<Q, F>(&self, key: &Q, predicate: F) -> Option<(K, V)>
    where
        Q: Hash + PartialEq,
        K: Borrow<Q> + Hash,
        F: FnOnce(&V) -> bool,
    {
        let buckets = self.inner.read();
        let ret = match buckets.entry(key) {
            Some(mut entry) => match entry.get() {
                Some((_, v)) if predicate(v) => mem::replace(&mut *entry, inner::Entry::Removed),
                _ => return None,
            },
            None => return None,
        };
        if !ret.is_free() {
            let len = self.len.fetch_sub(1, SeqCst) - 1;
            if len * GROW_FACTOR * LOAD_FACTOR_D < buckets.len() * LOAD_FACTOR_N {
                drop(buckets);
                self.shrink(len);
            }
        }
        ret.into()
    }

    #[inline]
    pub fn remove_entry<Q>(&self, key: &Q) -> Option<(K, V)>
    where
        Q: Hash + PartialEq,
        K: Borrow<Q> + Hash,
    {
        self.remove_entry_if(key, |_| true)
    }

    #[inline]
    pub fn remove_if<Q, F>(&self, key: &Q, predicate: F) -> Option<V>
    where
        Q: Hash + PartialEq,
        K: Borrow<Q> + Hash,
        F: FnOnce(&V) -> bool,
    {
        self.remove_entry_if(key, predicate).map(|(_, value)| value)
    }

    #[inline]
    pub fn remove<Q>(&self, key: &Q) -> Option<V>
    where
        Q: Hash + PartialEq,
        K: Borrow<Q> + Hash,
    {
        self.remove_entry(key).map(|ret| ret.1)
    }

    pub fn retain_mut<F>(&self, predicate: F)
    where
        F: Fn(&K, &mut V) -> bool,
    {
        let buckets = self.inner.read();
        for ent in buckets.as_inner() {
            let mut ent = ent.write();

            let remain = match *ent {
                inner::Entry::Data((ref key, ref mut value)) => predicate(key, value),
                _ => true,
            };
            if !remain {
                *ent = inner::Entry::Removed;
                self.len.fetch_sub(1, SeqCst);
            }
        }
    }

    #[inline]
    pub fn retain<F>(&self, predicate: F)
    where
        F: Fn(&K, &V) -> bool,
    {
        self.retain_mut(|key, value| predicate(key, value))
    }
}

impl<K, V, S: BuildHasher + Default> fmt::Debug for CHashMap<K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entry(&"..").finish()
    }
}

impl<K: Clone, V: Clone, S: Clone> Clone for CHashMap<K, V, S> {
    fn clone(&self) -> Self {
        Self {
            inner: RwLock::new(self.inner.read().clone()),
            len: self.len.load(SeqCst).into(),
        }
    }
}

pub struct IntoIter<K, V> {
    inner: alloc::vec::IntoIter<RwLock<inner::Entry<(K, V)>>>,
}

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        for ent in self.inner.by_ref() {
            if let inner::Entry::Data((key, value)) = ent.into_inner() {
                return Some((key, value));
            }
        }
        None
    }
}

impl<K, V, S> IntoIterator for CHashMap<K, V, S> {
    type Item = (K, V);

    type IntoIter = IntoIter<K, V>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            inner: self.inner.into_inner().into_inner().into_iter(),
        }
    }
}
