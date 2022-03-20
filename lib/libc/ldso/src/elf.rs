use core::mem;

#[inline(always)]
pub unsafe fn apply_relr(base: *mut u8, relr: *const usize, len: usize) {
    let len = len / mem::size_of::<usize>();

    let mut i = 0;
    while i < len {
        let addr = base.add(*relr.add(i)).cast::<usize>();
        i += 1;

        *addr += base as usize;

        let mut addr = addr.add(1);
        while i < len && *relr.add(i) & 1 != 0 {
            let mut run = addr;
            addr = addr.add(usize::BITS as usize - 1);

            let mut bitmask = *relr.add(i) >> 1;
            i += 1;
            while bitmask != 0 {
                let skip = bitmask.trailing_zeros() as usize;
                run = run.add(skip);
                *run += base as usize;
                run = run.add(1);
                bitmask >>= skip + 1;
            }
        }
    }
}
