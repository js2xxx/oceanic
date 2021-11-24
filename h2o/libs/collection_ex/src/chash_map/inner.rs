use alloc::vec::Vec;
use core::{
    borrow::Borrow,
    hash::{BuildHasher, Hash},
    iter,
};

use spin::{RwLock, RwLockReadGuard, RwLockWriteGuard};

pub enum Entry<T> {
    Empty,
    Data(T),
    Removed,
}

impl<T> Entry<T> {
    pub fn is_empty(&self) -> bool {
        matches!(self, Entry::Empty)
    }

    pub fn is_removed(&self) -> bool {
        matches!(self, Entry::Removed)
    }

    pub fn is_free(&self) -> bool {
        self.is_empty() || self.is_removed()
    }

    pub fn get(&self) -> Option<&T> {
        match self {
            Entry::Data(ref data) => Some(data),
            _ => None,
        }
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        match self {
            Entry::Data(ref mut data) => Some(data),
            _ => None,
        }
    }
}

impl<T> From<Entry<T>> for Option<T> {
    fn from(e: Entry<T>) -> Self {
        match e {
            Entry::Data(data) => Some(data),
            _ => None,
        }
    }
}

impl<K, V> Entry<(K, V)> {
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        Q: ?Sized + PartialEq,
        K: Borrow<Q>,
    {
        matches!(&self, Entry::Data((k, _)) if key == k.borrow())
    }

    pub fn key_or_end<Q>(&self, key: &Q) -> bool
    where
        Q: ?Sized + PartialEq,
        K: Borrow<Q>,
    {
        self.contains_key(key) || self.is_empty()
    }
}

pub struct Buckets<K, V, S> {
    hasher: S,
    data: Vec<RwLock<Entry<(K, V)>>>,
}

unsafe impl<K: Send, V: Send, S: Send> Send for Buckets<K, V, S> {}
unsafe impl<K: Sync + Send, V: Sync + Send, S> Sync for Buckets<K, V, S> {}

impl<K, V, S> Buckets<K, V, S> {
    pub fn with_capacity(hasher: S, capacity: usize) -> Self {
        let data = iter::repeat_with(|| RwLock::new(Entry::Empty))
            .take(capacity)
            .collect();
        Buckets { hasher, data }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

impl<K, V, S: BuildHasher> Buckets<K, V, S> {
    pub fn find_read<Q, F>(&self, key: &Q, predicate: F) -> Option<RwLockReadGuard<Entry<(K, V)>>>
    where
        Q: ?Sized + Hash,
        K: Borrow<Q>,
        F: Fn(&Entry<(K, V)>) -> bool,
    {
        let hash = self.hasher.hash_one(key) as usize;
        let len = self.data.len();
        for slot in self.data.iter().cycle().skip(hash % len).take(len) {
            let entry = slot.read();
            if predicate(&*entry) {
                return Some(entry);
            }
        }
        None
    }

    pub fn find_write<Q, F>(&self, key: &Q, predicate: F) -> Option<RwLockWriteGuard<Entry<(K, V)>>>
    where
        Q: ?Sized + Hash,
        K: Borrow<Q>,
        F: Fn(&Entry<(K, V)>) -> bool,
    {
        let hash = self.hasher.hash_one(key) as usize;
        let len = self.data.len();
        for slot in self.data.iter().cycle().skip(hash % len).take(len) {
            let entry = slot.write();
            if predicate(&*entry) {
                return Some(entry);
            }
        }
        None
    }

    pub fn find_mut<Q, F>(&mut self, key: &Q, predicate: F) -> Option<&mut Entry<(K, V)>>
    where
        Q: ?Sized + Hash,
        K: Borrow<Q>,
        F: Fn(&Entry<(K, V)>) -> bool,
    {
        let hash = self.hasher.hash_one(key) as usize;
        let len = self.data.len();
        for i in 0..len {
            let idx = (hash + i) % len;
            let entry = unsafe { self.data.get_unchecked_mut(idx) }.get_mut();
            if predicate(&*entry) {
                return Some(unsafe { self.data.get_unchecked_mut(idx) }.get_mut());
            }
        }
        None
    }

    pub fn entry<Q>(&self, key: &Q) -> Option<RwLockWriteGuard<Entry<(K, V)>>>
    where
        Q: ?Sized + Hash + PartialEq,
        K: Borrow<Q>,
    {
        let hash = self.hasher.hash_one(key) as usize;
        let len = self.data.len();
        let mut free = None;
        for slot in self.data.iter().cycle().skip(hash % len).take(len) {
            let entry = slot.write();
            match &*entry {
                Entry::Empty => return Some(free.unwrap_or(entry)),
                Entry::Data((ref k, _)) if k.borrow() == key => return Some(entry),
                Entry::Data(_) => {}
                Entry::Removed => free = free.or(Some(entry)),
            }
        }
        None
    }

    pub fn move_from(&mut self, other: Self)
    where
        K: Hash,
    {
        for slot in other.data {
            if let Entry::Data((key, value)) = slot.into_inner() {
                if let Some(entry) = self.find_mut(&key, |entry| entry.is_free()) {
                    *entry = Entry::Data((key, value));
                }
            }
        }
    }
}
