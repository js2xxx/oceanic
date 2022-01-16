use alloc::sync::Arc;
use core::{
    any::Any,
    fmt,
    iter::FusedIterator,
    marker::{PhantomData, PhantomPinned},
    mem,
    ops::Deref,
    ptr::NonNull,
};

use spin::Lazy;

use super::Object;
use crate::{mem::Arena, sched::PREEMPT};

pub const MAX_HANDLE_COUNT: usize = 1 << 18;

pub(super) static HR_ARENA: Lazy<Arena<Ref<dyn Any>>> = Lazy::new(|| Arena::new(MAX_HANDLE_COUNT));

#[derive(Debug)]
pub struct Ref<T: ?Sized> {
    obj: Arc<Object<T>>,
    next: Option<Ptr>,
    prev: Option<Ptr>,
    _marker: PhantomPinned,
}
pub type Ptr = NonNull<Ref<dyn Any>>;

unsafe impl<T: ?Sized> Send for Ref<T> {}

impl<T: 'static> Ref<T> {
    /// # Safety
    ///
    /// The caller must ensure that `T` is [`Send`] if `send` and [`Sync`] if
    /// `sync`.
    pub unsafe fn new_unchecked(data: T, send: bool, sync: bool) -> Self {
        Ref {
            obj: Arc::new(Object { send, sync, data }),
            next: None,
            prev: None,
            _marker: PhantomPinned,
        }
    }

    /// # Safety
    ///
    /// The caller must ensure that `self` is not inserted in any handle list.
    pub unsafe fn coerce_unchecked(self) -> Ref<dyn Any> {
        Ref {
            obj: self.obj,
            next: None,
            prev: None,
            _marker: PhantomPinned,
        }
    }
}

impl<T: ?Sized + 'static> Ref<T> {
    /// # Safety
    ///
    /// The caller must ensure that `self` is owned by the current task if its
    /// not [`Send`].
    pub unsafe fn deref_unchecked(&self) -> &T {
        &self.obj.data
    }
}

impl<T: Send + 'static> Ref<T> {
    pub fn new(data: T) -> Self {
        unsafe { Self::new_unchecked(data, true, false) }
    }
}

impl<T: ?Sized + Send + 'static> Deref for Ref<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: It's `Send`.
        unsafe { self.deref_unchecked() }
    }
}

impl<T: ?Sized + Send + Sync> Clone for Ref<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            obj: Arc::clone(&self.obj),
            next: None,
            prev: None,
            _marker: PhantomPinned,
        }
    }
}

impl Ref<dyn Any> {
    pub fn downcast_ref<T: Any>(&self) -> Option<&Ref<T>> {
        if self.obj.data.is::<T>() {
            Some(unsafe { &*(self as *const Ref<dyn Any> as *const Ref<T>) })
        } else {
            None
        }
    }

    /// # Safety
    ///
    /// The caller must ensure that every reference to the underlying object is
    /// not to be moved to another task if its not [`Send`] or [`Sync`].
    #[inline]
    #[must_use = "Don't make useless clonings"]
    pub unsafe fn clone_unchecked(&self) -> Ref<dyn Any> {
        Self {
            obj: Arc::clone(&self.obj),
            next: None,
            prev: None,
            _marker: PhantomPinned,
        }
    }

    pub fn try_clone(&self) -> Option<Ref<dyn Any>> {
        if self.obj.send && self.obj.sync {
            // SAFETY: The underlying object is `send` and `sync`.
            Some(unsafe { self.clone_unchecked() })
        } else {
            None
        }
    }

    #[inline]
    pub fn is_send(&self) -> bool {
        self.obj.send
    }
}

pub struct List {
    head: Option<Ptr>,
    tail: Option<Ptr>,
    len: usize,
    _marker: PhantomData<Ref<dyn Any>>,
}

unsafe impl Send for List {}

impl List {
    #[inline]
    pub fn new() -> Self {
        List {
            head: None,
            tail: None,
            len: 0,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for List {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl List {
    /// # Safety
    ///
    /// The caller must ensure that the pointers belongs to the list and `start`
    /// is some predecessor of `end` or equals `end`.
    unsafe fn splice_nodes(&mut self, mut start: Ptr, mut end: Ptr) {
        // These two are ours now, and we can create &mut s.
        let (start, end) = unsafe { (start.as_mut(), end.as_mut()) };

        // Not creating new mutable (unique!) references overlapping `element`.
        match start.prev {
            Some(mut prev) => unsafe { prev.as_mut().next = end.next },
            // These nodes start with the head.
            None => self.head = end.next,
        }

        match end.next {
            Some(mut next) => unsafe { next.as_mut().prev = start.prev },
            // These nodes end with the tail.
            None => self.tail = start.prev,
        }

        start.prev = None;
        end.next = None;
    }

    /// # Safety
    ///
    /// The caller must ensure that `link` doesn't belong to another list.
    unsafe fn insert_node(&mut self, mut link: Ptr) {
        // This one is ours now, and we can create a &mut.
        let value = unsafe { link.as_mut() };
        value.next = None;
        value.prev = self.tail;

        match self.tail {
            // SAFETY: If tail is not null, then tail is allocated from the arena.
            Some(mut tail) => unsafe { tail.as_mut().next = Some(link) },
            None => self.head = Some(link),
        }

        self.tail = Some(link);
    }
}

impl List {
    /// # Safety
    ///
    /// The caller must ensure that `value` comes from the current task if its
    /// not [`Send`].
    pub(super) unsafe fn insert_impl(&mut self, value: Ref<dyn Any>) -> Option<Ptr> {
        let link = HR_ARENA.allocate()?;
        // SAFETY: The pointer is allocated from the arena.
        unsafe { link.as_ptr().write(value) };

        self.insert_node(link);
        self.len += 1;

        Some(link)
    }

    /// # Safety
    ///
    /// The caller must ensure that the list belongs to the current task if
    /// `link` is not [`Send`].
    pub(super) unsafe fn remove_impl(&mut self, link: Ptr) -> Option<Ref<dyn Any>> {
        let mut cur = self.head;
        loop {
            cur = match cur {
                Some(cur) if cur == link => {
                    // SAFETY: The pointer is ours.
                    unsafe { self.splice_nodes(cur, cur) };
                    self.len -= 1;

                    // SAFETY: The pointer will be no longer read again and the ownership is moved
                    // to `value`.
                    let value = unsafe { cur.as_ptr().read() };
                    // SAFETY: The pointer is ours.
                    unsafe { HR_ARENA.deallocate(cur) };

                    break Some(value);
                }
                // SAFETY: The pointer is allocated from the arena.
                Some(cur) => unsafe { cur.as_ref().next },
                None => break None,
            }
        }
    }

    pub(super) fn split<I, F>(&mut self, iter: I, all: F) -> Option<List>
    where
        I: Iterator<Item = Option<Ptr>>,
        F: Fn(&Ref<dyn Any>) -> bool,
    {
        let mut ret = List::new();

        let mut cnt = 0;
        for ptr in iter {
            let link = match ptr {
                None => {
                    self.merge(&mut ret);
                    return None;
                }
                Some(link) if !all(unsafe { link.as_ref() }) => {
                    self.merge(&mut ret);
                    return None;
                }
                Some(link) => link,
            };
            unsafe {
                self.splice_nodes(link, link);
                ret.insert_node(link);
            }
            cnt += 1;
        }
        ret.len = cnt;
        self.len -= cnt;

        Some(ret)
    }

    pub(super) fn merge(&mut self, other: &mut List) -> Option<Iter> {
        let mut start = match other.head {
            Some(head) => head,
            None => return None,
        };
        let mut end = match other.tail {
            Some(tail) => tail,
            None => return None,
        };
        let list = mem::take(other);
        let len = list.len;
        mem::forget(list);

        let (start, end) = unsafe {
            start.as_mut().prev = self.tail;
            end.as_mut().next = None;
            (Some(start), Some(end))
        };

        match self.tail {
            // SAFETY: If tail is not null, then tail is allocated from the arena.
            Some(mut tail) => unsafe { tail.as_mut().next = start },
            None => self.head = start,
        }

        self.tail = end;
        self.len += len;

        Some(Iter {
            head: start,
            len,
            _marker: PhantomData,
        })
    }

    pub fn iter(&self) -> Iter {
        Iter {
            head: self.head,
            len: self.len,
            _marker: PhantomData,
        }
    }
}

impl fmt::Debug for List {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl Drop for List {
    fn drop(&mut self) {
        while let Some(head) = self.head {
            let _ = unsafe { self.remove_impl(head) };
        }
    }
}

pub struct Iter<'a> {
    head: Option<Ptr>,
    len: usize,
    _marker: PhantomData<&'a Ref<dyn Any>>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Ref<dyn Any>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            self.head.map(|head| unsafe {
                let ret = head.as_ref();
                self.head = ret.next;
                self.len -= 1;
                ret
            })
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'a> ExactSizeIterator for Iter<'a> {}

impl<'a> FusedIterator for Iter<'a> {}

#[inline]
pub fn decode(index: usize) -> Option<Ptr> {
    PREEMPT.scope(|| HR_ARENA.from_index(index))
}

#[inline]
pub fn encode(value: Ptr) -> Option<usize> {
    PREEMPT.scope(|| HR_ARENA.to_index(value))
}
