use alloc::{
    alloc::Global,
    collections::{btree_map::Entry, BTreeMap},
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{
    alloc::Allocator,
    mem,
    ptr::NonNull,
    slice,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

use archop::Azy;
use bitop_ex::BitOpEx;
use paging::{LAddr, PAddr, PAGE_LAYOUT, PAGE_SHIFT, PAGE_SIZE};
use spin::Mutex;
use sv_call::{
    ipc::{SIG_READ, SIG_WRITE},
    EAGAIN, EBUSY, EFAULT, ENOMEM, EPERM, ERANGE,
};

use super::PhysTrait;
use crate::{
    sched::{Arsc, BasicEvent, Event, PREEMPT},
    syscall::{In, Out, UserPtr},
};

static ZERO_PAGE: Azy<Page> = Azy::new(|| Page::allocate().unwrap());

#[derive(Debug)]
struct Page {
    base: PAddr,
    ptr: NonNull<u8>,
}

unsafe impl Send for Page {}
unsafe impl Sync for Page {}

impl Page {
    fn allocate() -> Option<Page> {
        let ptr = Global.allocate_zeroed(PAGE_LAYOUT).ok()?;
        let base = LAddr::from(ptr).to_paddr(minfo::ID_OFFSET);
        Some(Page {
            base,
            ptr: ptr.as_non_null_ptr(),
        })
    }

    fn copy_from(&mut self, addr: PAddr) {
        let src = addr.to_laddr(minfo::ID_OFFSET);
        unsafe {
            let ptr = self.ptr.as_ptr();
            ptr.copy_from_nonoverlapping(*src, PAGE_SIZE)
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        unsafe { Global.deallocate(self.ptr, PAGE_LAYOUT) }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Error {
    Alloc,
    WouldBlock,
    OutOfRange(usize),
    Pinned,
    MaxPinCount,
    Other(sv_call::Error),
}

impl From<Error> for sv_call::Error {
    fn from(value: Error) -> Self {
        match value {
            Error::Alloc => ENOMEM,
            Error::WouldBlock => EAGAIN,
            Error::OutOfRange(_) => ERANGE,
            Error::Pinned => EBUSY,
            Error::MaxPinCount => EFAULT,
            Error::Other(err) => err,
        }
    }
}

#[derive(Debug)]
enum Commit {
    Insert(Page),
    Ref(PAddr),
}

#[derive(Debug)]
enum PageState {
    ShouldCopy,
    ShouldMove,
}

#[derive(Debug)]
struct PageNode {
    state: PageState,
    page: Option<Page>,
    pin_count: usize,
}

impl PageNode {
    fn new(page: Page) -> Self {
        PageNode {
            state: PageState::ShouldCopy,
            page: Some(page),
            pin_count: 0,
        }
    }

    fn get_from_branch(&mut self, write: bool) -> Result<(Commit, bool), Error> {
        match self.state {
            PageState::ShouldCopy => {
                if write {
                    let mut page = Page::allocate().ok_or(Error::Alloc)?;
                    let src = self.page.as_ref().expect("the page has been moved");
                    page.copy_from(src.base);
                    self.state = PageState::ShouldMove;
                    Ok((Commit::Insert(page), false))
                } else {
                    let page = self.page.as_ref().expect("the page has been moved");
                    Ok((Commit::Ref(page.base), false))
                }
            }
            PageState::ShouldMove => {
                let page = self.page.take().expect("the page has been moved");
                Ok((Commit::Insert(page), true))
            }
        }
    }

    fn get_from_leaf(&mut self, write: bool) -> Result<PAddr, Error> {
        if let Some(ref page) = self.page {
            return Ok(page.base);
        }

        Ok(if write {
            let page = Page::allocate().ok_or(Error::Alloc)?;
            let base = page.base;
            self.page = Some(page);
            base
        } else {
            ZERO_PAGE.base
        })
    }
}

#[derive(Debug)]
struct PageList {
    branch: bool,

    parent: Option<Arsc<Phys>>,
    parent_start: usize,
    parent_end: usize,

    pages: BTreeMap<usize, PageNode>,
    count: usize,
    pin_count: usize,
}

#[derive(Debug)]
pub struct Phys {
    event: Arc<BasicEvent>,
    len: AtomicUsize,
    list: Mutex<PageList>,
}

impl PageList {
    fn commit_impl(&mut self, index: usize, write: bool) -> Result<Commit, Error> {
        if index >= self.count {
            return Err(Error::OutOfRange(index));
        }

        let ent = match self.pages.entry(index) {
            Entry::Vacant(ent) => ent,
            Entry::Occupied(mut ent) => {
                return Ok(if self.branch {
                    let (ret, should_remove) = ent.get_mut().get_from_branch(write)?;
                    if should_remove {
                        ent.remove();
                    }
                    ret
                } else {
                    Commit::Ref(ent.get_mut().get_from_leaf(write)?)
                })
            }
        };

        if let Some(parent) = self.parent.clone() {
            let mut list = parent.list.try_lock().ok_or(Error::WouldBlock)?;
            let parent_index = self.parent_start + index;
            if parent_index < self.parent_end {
                return match list.commit_impl(parent_index, write) {
                    Ok(Commit::Ref(base)) => Ok(Commit::Ref(base)),
                    Ok(Commit::Insert(page)) => {
                        let base = page.base;
                        ent.insert(PageNode::new(page));
                        Ok(Commit::Ref(base))
                    }
                    Err(err) => Err(err),
                };
            }
        }

        if !write {
            return Ok(Commit::Ref(ZERO_PAGE.base));
        }

        let page = Page::allocate().ok_or(Error::Alloc)?;
        Ok(if self.branch {
            Commit::Insert(page)
        } else {
            let base = page.base;
            ent.insert(PageNode::new(page));

            Commit::Ref(base)
        })
    }

    fn commit(&mut self, index: usize, write: bool) -> Result<PAddr, Error> {
        assert!(!self.branch);
        match self.commit_impl(index, write) {
            Ok(Commit::Ref(base)) => Ok(base),
            Ok(Commit::Insert(_)) => unreachable!(),
            Err(err) => Err(err),
        }
    }

    fn decommit(&mut self, index: usize) -> Result<(), Error> {
        if let Entry::Occupied(mut ent) = self.pages.entry(index) {
            if ent.get().pin_count > 0 {
                return Err(Error::Pinned);
            }
            if self.parent.is_some() {
                // Avoid getting a unowned copy from the parent again.
                ent.get_mut().page = None;
            } else {
                ent.remove();
            }
        }
        Ok(())
    }

    fn create_sub(&mut self, offset: usize, len: usize) -> Result<Phys, Error> {
        if self.pin_count > 0 {
            return Err(Error::Pinned);
        }
        let start = offset >> PAGE_SHIFT;
        let end = (offset + len).div_ceil_bit(PAGE_SHIFT);

        let branch = {
            let mut branch = Arsc::try_new_uninit().map_err(|_| Error::Alloc)?;
            unsafe {
                let uninit = Arsc::get_mut(&mut branch).unwrap();
                uninit.write(Phys {
                    event: BasicEvent::new(0),
                    len: AtomicUsize::new(0),
                    list: Mutex::new(PageList {
                        branch: true,
                        parent: self.parent.clone(),
                        parent_start: self.parent_start,
                        parent_end: self.parent_end,
                        pages: mem::take(&mut self.pages),
                        count: self.count,
                        pin_count: self.pin_count,
                    }),
                });
                Arsc::assume_init(branch)
            }
        };

        let sub = Phys {
            event: BasicEvent::new(0),
            len: AtomicUsize::new(len),
            list: Mutex::new(PageList {
                branch: false,
                parent: Some(branch.clone()),
                parent_start: start,
                parent_end: end,
                pages: BTreeMap::new(),
                count: end - start,
                pin_count: 0,
            }),
        };

        self.parent = Some(branch);
        self.parent_start = 0;
        self.parent_end = self.count;

        Ok(sub)
    }

    fn pin_impl(&mut self, index: usize, write: bool) -> Result<(), Error> {
        assert!(index < self.count, "Out of range");
        if let Some(node) = self.pages.get_mut(&index) {
            if node.pin_count >= isize::MAX as usize || self.pin_count >= isize::MAX as usize {
                return Err(Error::MaxPinCount);
            }
            node.pin_count += 1;
            self.pin_count += 1;
        } else if write {
            let parent = self.parent.clone().expect("Uncommitted page");
            let parent_index = self.parent_start + index;
            assert!(parent_index < self.parent_end, "Out of range");
            let mut list = parent.list.try_lock().ok_or(Error::WouldBlock)?;
            list.pin_impl(parent_index, write)?
        }
        Ok(())
    }

    fn pin(&mut self, start: usize, end: usize, write: bool) -> Result<Vec<(PAddr, usize)>, Error> {
        let bases = (start..end)
            .map(|index| self.commit(index, write).map(|base| (base, PAGE_SIZE)))
            .collect::<Result<Vec<_>, _>>()?;
        for index in start..end {
            if let Err(err) = self.pin_impl(index, write) {
                for index in start..index {
                    self.unpin_impl(index);
                }
                return Err(err);
            }
        }
        Ok(bases)
    }

    fn unpin_impl(&mut self, index: usize) {
        assert!(index < self.count, "Out of range");
        if let Some(node) = self.pages.get_mut(&index) {
            node.pin_count = node.pin_count.saturating_sub(1);
            self.pin_count = self.pin_count.saturating_sub(1);
        }
    }

    fn unpin(&mut self, start: usize, end: usize) {
        for index in start..end {
            self.unpin_impl(index)
        }
    }

    fn resize(&mut self, new_count: usize) -> Result<(), Error> {
        if self.pin_count > 0 {
            return Err(Error::Pinned);
        }
        if new_count < self.count {
            for index in self.count..new_count {
                let _ = self.decommit(index);
            }
        }
        self.count = new_count;

        Ok(())
    }
}

impl Phys {
    pub fn new(len: usize) -> Self {
        Phys {
            event: BasicEvent::new(0),
            len: AtomicUsize::new(len),
            list: Mutex::new(PageList {
                branch: false,
                parent: None,
                parent_start: 0,
                parent_end: 0,
                pages: BTreeMap::new(),
                count: len.div_ceil_bit(PAGE_SHIFT),
                pin_count: 0,
            }),
        }
    }

    pub fn read(&self, pos: usize, len: usize, buffer: UserPtr<Out>) -> Result<usize, Error> {
        let self_len = self.len.load(SeqCst);
        let pos = pos.min(self_len);
        let len = (self_len - pos).min(len);

        let mut list = self.list.try_lock().ok_or(Error::WouldBlock)?;
        let mut read_len = 0;

        let start = pos >> PAGE_SHIFT;
        let end = (pos + len).div_ceil_bit(PAGE_SHIFT);
        let mut pos_in_page = pos - (start << PAGE_SHIFT);
        for base in (start..end).map(|index| list.commit(index, false)) {
            match base {
                Ok(base) => unsafe {
                    let src = base.to_laddr(minfo::ID_OFFSET);
                    let src = LAddr::from(src.val() + pos_in_page);
                    let len = (len - read_len).min(PAGE_SIZE);

                    let buffer = UserPtr::<Out>::new(buffer.as_ptr().add(read_len));
                    let src = slice::from_raw_parts(*src, len);
                    buffer.write_slice(src).map_err(Error::Other)?;

                    read_len += len;
                    pos_in_page = 0;
                },
                Err(err) => log::warn!("read error: {err:?}"),
            }
        }
        Ok(read_len)
    }

    pub fn write(&self, pos: usize, len: usize, buffer: UserPtr<In>) -> Result<usize, Error> {
        let self_len = self.len.load(SeqCst);
        let pos = pos.min(self_len);
        let len = (self_len - pos).min(len);

        let mut list = self.list.try_lock().ok_or(Error::WouldBlock)?;
        let mut written_len = 0;

        let start = pos >> PAGE_SHIFT;
        let end = (pos + len).div_ceil_bit(PAGE_SHIFT);
        let mut pos_in_page = pos - (start << PAGE_SHIFT);
        for base in (start..end).map(|index| list.commit(index, true)) {
            match base {
                Ok(base) => unsafe {
                    let src = base.to_laddr(minfo::ID_OFFSET);
                    let src = LAddr::from(src.val() + pos_in_page);
                    let len = (len - written_len).min(PAGE_SIZE);

                    let buffer = UserPtr::<In>::new(buffer.as_ptr().add(written_len));
                    buffer.read_slice(*src, len).map_err(Error::Other)?;

                    written_len += len;
                    pos_in_page = 0;
                },
                Err(err) => log::warn!("read error: {err:?}"),
            }
        }
        Ok(written_len)
    }

    // pub fn commit(&self, start: usize, end: usize, write: bool) -> Result<(),
    // Error> {     let mut list =
    // self.list.try_lock().ok_or(Error::WouldBlock)?;     (start..end).
    // try_for_each(|index| list.commit(index, write).map(drop)) }

    // pub fn decommit(&self, start: usize, end: usize) -> Result<(), Error> {
    //     let mut list = self.list.try_lock().ok_or(Error::WouldBlock)?;
    //     (start..end).for_each(|index| {
    //         if let Err(err) = list.decommit(index) {
    //             log::warn!("decommit error: {err:?}")
    //         }
    //     });
    //     Ok(())
    // }

    pub fn create_sub(&self, offset: usize, len: usize) -> Result<Phys, Error> {
        self.list
            .try_lock()
            .ok_or(Error::WouldBlock)?
            .create_sub(offset, len)
    }

    pub fn resize(&self, new_len: usize) -> Result<(), Error> {
        let new_count = new_len.div_ceil_bit(PAGE_SHIFT);
        self.list
            .try_lock()
            .ok_or(Error::WouldBlock)?
            .resize(new_count)?;
        self.len.store(new_len, SeqCst);
        Ok(())
    }
}

impl PhysTrait for Phys {
    #[inline]
    fn event(&self) -> Weak<dyn Event> {
        Arc::downgrade(&self.event) as _
    }

    #[inline]
    fn len(&self) -> usize {
        self.len.load(SeqCst)
    }

    #[inline]
    fn pin(&self, offset: usize, len: usize, write: bool) -> sv_call::Result<Vec<(PAddr, usize)>> {
        let start = offset >> PAGE_SHIFT;
        let end = (offset + len).div_ceil_bit(PAGE_SHIFT);
        let ret = PREEMPT.scope(|| self.list.lock().pin(start, end, write))?;
        self.event.notify(0, SIG_READ | SIG_WRITE);
        Ok(ret)
    }

    #[inline]
    fn unpin(&self, offset: usize, len: usize) {
        let start = offset >> PAGE_SHIFT;
        let end = (offset + len).div_ceil_bit(PAGE_SHIFT);
        PREEMPT.scope(|| self.list.lock().unpin(start, end));
        self.event.notify(0, SIG_READ | SIG_WRITE);
    }

    fn create_sub(
        &self,
        offset: usize,
        len: usize,
        copy: bool,
    ) -> sv_call::Result<Arc<super::Phys>> {
        if !copy {
            return Err(EPERM);
        }
        let mut ret = Arc::try_new_uninit()?;
        let sub = Arc::get_mut(&mut ret).unwrap();
        let value = self.create_sub(offset, len)?;
        self.event.notify(0, SIG_READ | SIG_WRITE);
        sub.write(value.into());
        Ok(unsafe { ret.assume_init() })
    }

    #[inline]
    fn base(&self) -> PAddr {
        unimplemented!("Extensible phys have multiple bases")
    }

    #[inline]
    fn resize(&self, new_len: usize, _: bool) -> sv_call::Result {
        self.resize(new_len)?;
        self.event.notify(0, SIG_READ | SIG_WRITE);
        Ok(())
    }

    #[inline]
    fn read(&self, offset: usize, len: usize, buffer: UserPtr<Out>) -> sv_call::Result<usize> {
        let ret = self.read(offset, len, buffer)?;
        self.event.notify(0, SIG_READ | SIG_WRITE);
        Ok(ret)
    }

    #[inline]
    fn write(&self, offset: usize, len: usize, buffer: UserPtr<In>) -> sv_call::Result<usize> {
        let ret = self.write(offset, len, buffer)?;
        self.event.notify(0, SIG_READ | SIG_WRITE);
        Ok(ret)
    }
}

impl PartialEq for Phys {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.event, &other.event)
    }
}
