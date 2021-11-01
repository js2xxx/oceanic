use core::marker::PhantomData;

pub struct PointerIterator<T> {
    ptr: *mut u8,
    len: usize,
    step: usize,
    _t: PhantomData<T>,
}

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
