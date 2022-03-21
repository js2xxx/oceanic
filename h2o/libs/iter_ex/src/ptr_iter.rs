use core::{marker::PhantomData, ptr::null_mut};

#[derive(Debug, Clone, Copy)]
pub struct PointerIterator<T> {
    ptr: *mut u8,
    len: usize,
    step: usize,
    _t: PhantomData<T>,
}

unsafe impl<T: Send> Send for PointerIterator<T> {}
unsafe impl<T: Sync> Sync for PointerIterator<T> {}

impl<T> PointerIterator<T> {
    pub fn new(ptr: *mut T, len: usize, step: usize) -> Self {
        PointerIterator {
            ptr: ptr.cast(),
            len,
            step,
            _t: PhantomData,
        }
    }
}

impl<T> Iterator for PointerIterator<T> {
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

impl<T> ExactSizeIterator for PointerIterator<T> {}

impl<'a, T: Copy> IntoIterator for &'a PointerIterator<T> {
    type Item = *mut T;

    type IntoIter = PointerIterator<T>;

    fn into_iter(self) -> Self::IntoIter {
        *self
    }
}

impl<T> Default for PointerIterator<T> {
    fn default() -> Self {
        Self {
            ptr: null_mut(),
            len: 0,
            step: 1,
            _t: PhantomData,
        }
    }
}
