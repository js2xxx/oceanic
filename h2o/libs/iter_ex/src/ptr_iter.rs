use core::{marker::PhantomData, ptr::null_mut};

#[derive(Clone, Copy)]
pub struct PtrIter<T> {
    ptr: *mut u8,
    len: usize,
    step: usize,
    _t: PhantomData<T>,
}

unsafe impl<T: Send> Send for PtrIter<T> {}
unsafe impl<T: Sync> Sync for PtrIter<T> {}

impl<T> PtrIter<T> {
    pub fn new(ptr: *mut T, len: usize, step: usize) -> Self {
        PtrIter {
            ptr: ptr.cast(),
            len,
            step,
            _t: PhantomData,
        }
    }

    pub fn new_size(ptr: *mut T, size: usize, step: usize) -> Self {
        Self::new(ptr, size / step, step)
    }

    pub fn pointer(&self) -> *mut T {
        self.ptr.cast()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn size(&self) -> usize {
        self.len * self.step
    }

    pub fn step(&self) -> usize {
        self.step
    }

    pub fn get(&self, index: usize) -> Option<*mut T> {
        (index < self.len).then(|| unsafe { self.ptr.add(index * self.step) }.cast())
    }
}

impl<T> Iterator for PtrIter<T> {
    type Item = *mut T;

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }

    fn next(&mut self) -> Option<Self::Item> {
        if self.len > 0 {
            let ret = self.ptr;

            self.ptr = unsafe { self.ptr.add(self.step) };
            self.len -= 1;

            Some(ret.cast())
        } else {
            None
        }
    }
}

impl<T> ExactSizeIterator for PtrIter<T> {}

impl<'a, T: Copy> IntoIterator for &'a PtrIter<T> {
    type Item = *mut T;

    type IntoIter = PtrIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        *self
    }
}

impl<T> Default for PtrIter<T> {
    fn default() -> Self {
        Self {
            ptr: null_mut(),
            len: 0,
            step: 1,
            _t: PhantomData,
        }
    }
}

impl<T> core::fmt::Debug for PtrIter<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:p} [..{:?}].step({:?})", self.ptr, self.len, self.step)
    }
}
