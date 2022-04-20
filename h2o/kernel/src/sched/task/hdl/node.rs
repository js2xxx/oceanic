use alloc::sync::{Weak, Arc};
use core::{
    any::Any,
    fmt,
    iter::FusedIterator,
    marker::{PhantomData, PhantomPinned, Unsize},
    mem,
    ops::{CoerceUnsized, Deref},
    ptr::NonNull,
};

use archop::Azy;
use sv_call::{Feature, Result};

use super::DefaultFeature;
use crate::{
    mem::Arena,
    sched::{Event, PREEMPT},
};

pub const MAX_HANDLE_COUNT: usize = 1 << 16;

pub(super) static HR_ARENA: Azy<Arena<Ref>> = Azy::new(|| Arena::new(MAX_HANDLE_COUNT));

#[derive(Debug)]
pub struct Ref<T: ?Sized = dyn Any + Send + Sync> {
    _marker: PhantomPinned,
    next: Option<Ptr>,
    prev: Option<Ptr>,
    event: Weak<dyn Event>,
    feat: Feature,
    obj: Arc<T>,
}
pub type Ptr = NonNull<Ref>;

unsafe impl<T: ?Sized> Send for Ref<T> {}

impl<T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<Ref<U>> for Ref<T> {}

impl<T: ?Sized> Ref<T> {
    /// # Safety
    ///
    /// The caller must ensure that `T` is [`Send`] if `send` and [`Sync`] if
    /// `sync`.
    pub unsafe fn try_new_unchecked(
        data: T,
        feat: Feature,
        event: Option<Weak<dyn Event>>,
    ) -> sv_call::Result<Self>
    where
        T: Sized,
    {
        Self::from_raw_unchecked(Arc::try_new(data)?, feat, event)
    }

    /// # Safety
    ///
    /// The caller must ensure that `T` is [`Send`] if `send` and [`Sync`] if
    /// `sync`.
    pub unsafe fn from_raw_unchecked(
        obj: Arc<T>,
        feat: Feature,
        event: Option<Weak<dyn Event>>,
    ) -> sv_call::Result<Self> {
        let event = event.unwrap_or(Weak::<crate::sched::BasicEvent>::new() as _);
        if event.strong_count() == 0 && feat.contains(Feature::WAIT) {
            return Err(sv_call::EPERM);
        }
        Ok(Ref {
            _marker: PhantomPinned,
            next: None,
            prev: None,
            event,
            feat,
            obj,
        })
    }

    #[inline]
    pub fn try_new(data: T, event: Option<Weak<dyn Event>>) -> sv_call::Result<Self>
    where
        T: DefaultFeature + Sized,
    {
        unsafe { Self::try_new_unchecked(data, T::default_features(), event) }
    }

    #[inline]
    pub fn from_raw(obj: Arc<T>, event: Option<Weak<dyn Event>>) -> sv_call::Result<Self>
    where
        T: DefaultFeature,
    {
        unsafe { Self::from_raw_unchecked(obj, T::default_features(), event) }
    }

    #[inline]
    pub fn into_raw(this: Self) -> Arc<T> {
        this.obj
    }

    /// # Safety
    ///
    /// The caller must ensure that `self` is owned by the current task if its
    /// not [`Send`].
    pub unsafe fn deref_unchecked(&self) -> &Arc<T> {
        &self.obj
    }

    #[inline]
    pub fn event(&self) -> &Weak<dyn Event> {
        &self.event
    }

    #[inline]
    pub fn features(&self) -> Feature {
        self.feat
    }

    pub fn set_features(&mut self, feat: Feature) -> Result {
        if feat & !self.feat == Feature::empty() {
            self.feat = feat;
            Ok(())
        } else {
            Err(sv_call::EPERM)
        }
    }
}

impl<T: ?Sized + Send + Sync> Deref for Ref<T> {
    type Target = Arc<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: It's `Send`.
        unsafe { self.deref_unchecked() }
    }
}

impl Ref {
    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        self.obj.is::<T>()
    }

    pub fn downcast_ref<T: Any>(&self) -> Result<&Ref<T>> {
        if self.is::<T>() {
            Ok(unsafe { &*(self as *const Ref as *const Ref<T>) })
        } else {
            Err(sv_call::ETYPE)
        }
    }

    pub fn downcast<T: Any + Send + Sync>(self) -> core::result::Result<Ref<T>, Self> {
        match self.obj.downcast() {
            Ok(obj) => Ok(Ref {
                _marker: PhantomPinned,
                next: None,
                prev: None,
                event: self.event,
                feat: self.feat,
                obj,
            }),
            Err(obj) => Err(Ref {
                _marker: PhantomPinned,
                next: None,
                prev: None,
                event: self.event,
                feat: self.feat,
                obj,
            }),
        }
    }

    /// # Safety
    ///
    /// The caller must ensure that every reference to the underlying object is
    /// not to be moved to another task if its not [`Send`] or [`Sync`].
    #[inline]
    #[must_use = "Don't make useless clonings"]
    unsafe fn clone_unchecked(&self) -> Ref {
        Ref {
            _marker: PhantomPinned,
            next: None,
            prev: None,
            event: Weak::clone(&self.event),
            feat: self.feat,
            obj: Arc::clone(&self.obj),
        }
    }

    pub fn try_clone(&self) -> Result<Ref> {
        let feat = self.features();
        if feat.contains(Feature::SEND | Feature::SYNC) {
            // SAFETY: The underlying object is `send` and `sync`.
            Ok(unsafe { self.clone_unchecked() })
        } else {
            Err(sv_call::EPERM)
        }
    }
}

pub struct List {
    head: Option<Ptr>,
    tail: Option<Ptr>,
    len: usize,
    _marker: PhantomData<Ref>,
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
    pub fn insert(&mut self, value: Ref) -> Result<Ptr> {
        let link = HR_ARENA.allocate()?;
        // SAFETY: The pointer is allocated from the arena.
        unsafe { link.as_ptr().write(value) };

        // SAFETY: The node is freshly allocated.
        unsafe { self.insert_node(link) };
        self.len += 1;

        Ok(link)
    }

    pub fn remove(&mut self, link: Ptr) -> Result<Ref> {
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
                    let _ = unsafe { HR_ARENA.deallocate(cur) };

                    break Ok(value);
                }
                // SAFETY: The pointer is allocated from the arena.
                Some(cur) => unsafe { cur.as_ref().next },
                None => break Err(sv_call::ENOENT),
            }
        }
    }

    pub(super) fn split<I, F>(&mut self, iter: I, check: F) -> Result<List>
    where
        I: Iterator<Item = Result<Ptr>>,
        F: Fn(&Ref) -> Result,
    {
        let mut ret = List::new();

        let mut cnt = 0;
        for ptr in iter {
            let link = match ptr {
                Err(err) => {
                    self.merge(&mut ret);
                    return Err(err);
                }
                Ok(link) => match check(unsafe { link.as_ref() }) {
                    Ok(()) => link,
                    Err(err) => {
                        self.merge(&mut ret);
                        return Err(err);
                    }
                },
            };
            unsafe {
                self.splice_nodes(link, link);
                ret.insert_node(link);
            }
            cnt += 1;
        }
        ret.len = cnt;
        self.len -= cnt;

        Ok(ret)
    }

    pub(super) fn merge(&mut self, other: &mut List) -> Iter {
        let mut start = match other.head {
            Some(head) => head,
            None => return Iter::empty(),
        };
        let mut end = match other.tail {
            Some(tail) => tail,
            None => return Iter::empty(),
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

        Iter {
            head: start,
            len,
            _marker: PhantomData,
        }
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
            let _ = self.remove(head);
        }
    }
}

pub struct Iter<'a> {
    head: Option<Ptr>,
    len: usize,
    _marker: PhantomData<&'a Ref>,
}

impl<'a> Iter<'a> {
    #[inline]
    pub fn empty() -> Self {
        Iter {
            head: None,
            len: 0,
            _marker: PhantomData,
        }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Ref;

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
pub fn decode(index: usize) -> Result<Ptr> {
    PREEMPT.scope(|| HR_ARENA.get_ptr(index))
}

#[inline]
pub fn encode(value: Ptr) -> Result<usize> {
    PREEMPT.scope(|| HR_ARENA.get_index(value))
}
