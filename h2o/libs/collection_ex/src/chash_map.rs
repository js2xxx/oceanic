mod inner;

use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash};
use core::mem;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering::*};

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
      _buckets: RwLockReadGuard<'a, inner::Buckets<K, V, S>>,
      inner: RwLockWriteGuard<'a, inner::Entry<(K, V)>>,
}

impl<'a, K, V, S> WriteGuard<'a, K, V, S> {
      pub fn key(&self) -> &K {
            &self.inner.get().unwrap().0
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

unsafe impl<K: Send, V: Send, S> Send for CHashMap<K, V, S> {}
unsafe impl<K: Sync + Send, V: Sync + Send, S> Sync for CHashMap<K, V, S> {}

impl<K, V, S> CHashMap<K, V, S> {
      pub fn new(hasher: S) -> Self {
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

impl<K, V, S: BuildHasher + Default> CHashMap<K, V, S> {
      fn grow(&self, new_len: usize)
      where
            K: Hash,
      {
            let len = new_len * GROW_FACTOR;
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
                  _buckets: buckets,
                  inner: entry,
            })
      }

      pub fn insert(&self, key: K, value: V) -> Option<(K, V)>
      where
            K: Hash + PartialEq,
      {
            let buckets = self.inner.read();
            let old = {
                  let mut entry = buckets.entry(&key);
                  mem::replace(&mut *entry, inner::Entry::Data((key, value)))
            };
            if old.is_free() {
                  let len = self.len.fetch_add(1, SeqCst) + 1;
                  if len * LOAD_FACTOR_D >= buckets.len() * LOAD_FACTOR_N {
                        drop(buckets);
                        self.grow(len);
                  }
            }
            old.into()
      }

      pub fn remove_entry<Q>(&self, key: &Q) -> Option<(K, V)>
      where
            Q: Hash + PartialEq,
            K: Borrow<Q> + Hash,
      {
            let buckets = self.inner.read();
            let ret = {
                  let mut entry = buckets.entry(key);
                  mem::replace(&mut *entry, inner::Entry::Removed)
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

      pub fn remove<Q>(&self, key: &Q) -> Option<V>
      where
            Q: Hash + PartialEq,
            K: Borrow<Q> + Hash,
      {
            self.remove_entry(key).map(|ret| ret.1)
      }
}
