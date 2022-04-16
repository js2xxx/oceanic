use alloc::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};
use core::{alloc::Layout, mem, ops::Range};

use bitop_ex::BitOpEx;
use paging::{LAddr, PAddr, PAGE_SHIFT, PAGE_SIZE};
use spin::Mutex;
use sv_call::{error::*, mem::Flags, Feature, Result};

use super::{paging_error, ty_to_range, Phys, Space};
use crate::sched::{
    task,
    task::{hdl::DefaultFeature, VDSO},
    PREEMPT,
};

#[derive(Debug)]
pub(super) enum Child {
    Virt(Arc<Virt>),
    Phys(Phys, Flags, usize),
}

impl Child {
    fn len(&self) -> usize {
        match self {
            Child::Virt(virt) => virt.len(),
            Child::Phys(_, _, len) => *len,
        }
    }

    pub(super) fn end(&self, base: LAddr) -> LAddr {
        LAddr::from(base.val() + self.len())
    }
}

type ChildMap = BTreeMap<LAddr, Child>;

#[derive(Debug)]
pub struct Virt {
    ty: task::Type,

    range: Range<LAddr>,
    pub(super) space: Weak<Space>,

    parent: Weak<Virt>,
    pub(super) children: Mutex<ChildMap>,
}

unsafe impl Send for Virt {}
unsafe impl Sync for Virt {}

impl Virt {
    pub(super) fn new_root(ty: task::Type, space: Weak<Space>) -> Arc<Self> {
        let range = ty_to_range(ty);
        Arc::new(Virt {
            ty,
            range: LAddr::from(range.start)..LAddr::from(range.end),
            space,
            parent: Weak::new(),
            children: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn range(&self) -> &Range<LAddr> {
        &self.range
    }

    pub fn len(&self) -> usize {
        self.range.end.val() - self.range.start.val()
    }

    pub fn is_empty(&self) -> bool {
        self.range.end.val() == self.range.start.val()
    }

    pub fn allocate(self: &Arc<Self>, offset: Option<usize>, layout: Layout) -> Result<Weak<Self>> {
        let layout = check_layout(layout)?;

        let _pree = PREEMPT.lock();
        let mut children = self.children.lock();

        let range = find_range(&children, &self.range, offset, layout)?;
        let base = range.start;

        let child = Arc::new(Virt {
            ty: self.ty,
            range,
            space: Weak::clone(&self.space),
            parent: Arc::downgrade(self),
            children: Mutex::new(BTreeMap::new()),
        });
        let ret = Arc::downgrade(&child);
        let _ = children.insert(base, Child::Virt(child));
        Ok(ret)
    }

    pub fn destroy(&self) -> Result {
        if let Some(space) = self.space.upgrade() {
            let _pree = PREEMPT.lock();
            let vdso = *space.vdso.lock();
            let children = self.children.lock();

            if { children.iter() }.any(|(&base, child)| !check_vdso(vdso, base, child.end(base))) {
                return Err(EACCES);
            }
        }
        if let Some(parent) = self.parent.upgrade() {
            let _ = PREEMPT.scope(|| parent.children.lock().remove(&self.range.start));
        }
        Ok(())
    }

    pub fn map(
        &self,
        offset: Option<usize>,
        phys: Phys,
        phys_offset: usize,
        layout: Layout,
        flags: Flags,
    ) -> Result<LAddr> {
        if phys == VDSO.1
            && (offset.is_some()
                || phys_offset != 0
                || layout.size() != VDSO.1.len()
                || layout.align() != PAGE_SIZE
                || flags != VDSO.0)
        {
            return Err(EACCES);
        }

        let layout = check_layout(layout)?;
        if phys_offset.contains_bit(PAGE_SHIFT) {
            return Err(EALIGN);
        }
        let phys_end = phys_offset.wrapping_add(layout.size());
        if !(phys_offset < phys_end && phys_end <= phys.len()) {
            return Err(ERANGE);
        }

        let _pree = PREEMPT.lock();
        let mut children = self.children.lock();
        let space = self.space.upgrade().ok_or(EKILLED)?;

        let set_vdso = phys == VDSO.1;
        if set_vdso {
            check_set_vdso(&space.vdso, phys_offset, layout, flags)?;
            if self as *const _ != Arc::as_ptr(&space.root) {
                return Err(EACCES);
            }
        }
        let virt = find_range(&children, &self.range, offset, layout)?;
        let base = virt.start;

        let phys_base = PAddr::new(*phys.base() + phys_offset);
        let _ = children.insert(base, Child::Phys(phys, flags, layout.size()));

        space.arch.maps(virt, phys_base, flags).map_err(|err| {
            let _ = children.remove(&base);
            paging_error(err)
        })?;

        if set_vdso {
            *space.vdso.lock() = Some(base);
        }
        Ok(base)
    }

    pub fn reprotect(&self, base: LAddr, len: usize, flags: Flags) -> Result {
        let start = base;
        let end = LAddr::from(base.val() + len);

        if !(self.range.start <= start && end <= self.range.end) {
            return Err(ERANGE);
        }

        let _pree = PREEMPT.lock();
        let children = self.children.lock();
        let space = self.space.upgrade().ok_or(EKILLED)?;

        let vdso = { *space.vdso.lock() };
        for (&base, child) in children
            .range(..end)
            .take_while(|(&base, child)| start <= child.end(base))
        {
            let child_end = child.end(base);
            if !(start <= base && child_end <= end) {
                return Err(ERANGE);
            }
            if !check_vdso(vdso, base, child_end) {
                return Err(EACCES);
            }
            match child {
                Child::Virt(_) => return Err(EINVAL),
                Child::Phys(_, f, _) if flags.intersects(!*f) => {
                    return Err(EPERM);
                }
                _ => {}
            }
        }

        for (&base, child) in children
            .range(start..)
            .take_while(|(&base, child)| child.end(base) <= end)
        {
            { space.arch.reprotect(base..child.end(base), flags) }.map_err(paging_error)?;
        }

        Ok(())
    }

    pub fn unmap(&self, base: LAddr, len: usize, drop_child: bool) -> Result {
        let start = base;
        let end = LAddr::from(base.val() + len);

        if !(self.range.start <= start && end <= self.range.end) {
            return Err(ERANGE);
        }

        let _pree = PREEMPT.lock();
        let mut children = self.children.lock();
        let space = self.space.upgrade().ok_or(EKILLED)?;

        let vdso = { *space.vdso.lock() };
        for (&base, child) in children
            .range(..end)
            .take_while(|(&base, child)| start <= child.end(base))
        {
            let child_end = child.end(base);
            if !(start <= base && child_end <= end) {
                return Err(ERANGE);
            }
            if !check_vdso(vdso, base, child_end) {
                return Err(EACCES);
            }
            if matches!(child, Child::Virt(_) if !drop_child) {
                return Err(EPERM);
            }
        }

        let mut mid = children.split_off(&start);
        let mut prefix = mid.split_off(&end);
        children.append(&mut prefix);
        drop(children);

        let mut ret = Ok(None);
        for (base, child) in mid {
            let end = child.end(base);
            if let Child::Phys(..) = child {
                let r = space.arch.unmaps(base..end);
                ret = ret.and(r.map_err(paging_error));
            }
        }

        ret.map(|_| {})
    }
}

impl Drop for Virt {
    fn drop(&mut self) {
        let children = mem::take(self.children.get_mut());
        if let Some(space) = self.space.upgrade() {
            for (base, child) in children {
                let end = child.end(base);
                if let Child::Phys(..) = child {
                    let _ = PREEMPT.scope(|| space.arch.unmaps(base..end));
                }
            }
        }
    }
}

unsafe impl DefaultFeature for Weak<Virt> {
    fn default_features() -> Feature {
        Feature::SYNC | Feature::READ | Feature::WRITE | Feature::EXECUTE
    }
}

impl PartialEq for Virt {
    fn eq(&self, other: &Self) -> bool {
        self.range == other.range && Weak::ptr_eq(&self.space, &other.space)
    }
}

impl Eq for Virt {}

impl PartialOrd for Virt {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.range.start.partial_cmp(&other.range.start)
    }
}

impl Ord for Virt {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.range.start.cmp(&other.range.start)
    }
}

fn check_layout(layout: Layout) -> Result<Layout> {
    if layout.size() == 0 {
        return Err(ERANGE);
    }
    if layout.align() < PAGE_SIZE {
        return Err(EALIGN);
    }
    Ok(layout.pad_to_align())
}

fn check_set_vdso(
    vdso: &Mutex<Option<LAddr>>,
    phys_offset: usize,
    layout: Layout,
    flags: Flags,
) -> Result {
    if PREEMPT.scope(|| vdso.lock().is_some()) {
        return Err(EACCES);
    }

    if phys_offset != 0 {
        return Err(EACCES);
    }

    if layout.size() != VDSO.1.len() || layout.align() != PAGE_SIZE {
        return Err(EACCES);
    }

    if flags != VDSO.0 {
        return Err(EACCES);
    }

    Ok(())
}

fn check_vdso(vdso: Option<LAddr>, base: LAddr, end: LAddr) -> bool {
    let vdso_size = VDSO.1.len();

    match vdso {
        None => true,
        Some(vdso_base) if end <= vdso_base || LAddr::from(vdso_base.val() + vdso_size) <= base => {
            true
        }
        _ => false,
    }
}

fn find_range(
    map: &BTreeMap<LAddr, Child>,
    range: &Range<LAddr>,
    offset: Option<usize>,
    layout: Layout,
) -> Result<Range<LAddr>> {
    let base = match offset {
        Some(offset) => {
            let base = LAddr::from({ range.start.val() }.checked_add(offset).ok_or(ERANGE)?);
            let end = LAddr::from(base.val().checked_add(layout.size()).ok_or(ERANGE)?);
            if base.val().contains_bit(PAGE_SHIFT) {
                return Err(EALIGN);
            }
            if !(range.start <= base && end <= range.end) {
                return Err(ERANGE);
            }
            if !check_alloc(map, base..end) {
                return Err(EEXIST);
            }
            base
        }
        None => find_alloc(map, range, layout).ok_or(ENOMEM)?,
    };

    Ok(base..LAddr::from(base.val() + layout.size()))
}

fn check_alloc(map: &ChildMap, request: Range<LAddr>) -> bool {
    let prev = map.range(..request.end).next();
    !matches!(prev, Some((&base, prev)) if prev.end(base) > request.start)
}

#[inline]
fn find_alloc(map: &ChildMap, range: &Range<LAddr>, layout: Layout) -> Option<LAddr> {
    const ASLR_BIT: usize = 35;
    let mask = (1 << ASLR_BIT) - 1;
    let (ret, cnt) = try_find_alloc(map, range, layout, rand() & mask);
    ret.or_else(|| try_find_alloc(map, range, layout, rand() % cnt).0)
}

#[inline]
fn rand() -> usize {
    archop::rand::get() as usize
}

fn try_find_alloc(
    map: &ChildMap,
    range: &Range<LAddr>,
    layout: Layout,
    mut rand_n: usize,
) -> (Option<LAddr>, usize) {
    let mut cnt = 0;
    let bit = layout.align().msb();
    let ret = gaps(map, range, |gap| {
        let (base, end) = (gap.start.val(), gap.end.val());
        let base = base.round_up_bit(bit);
        let end = end.round_down_bit(bit);
        if layout.size() <= end - base {
            let n = ((end - base - layout.size()) >> bit) + 1;
            cnt += n;
            if rand_n < n {
                return Some(LAddr::from(base + (rand_n << bit)));
            }
            rand_n -= n;
        }
        None
    });
    (ret, cnt)
}

fn gaps<F, R>(map: &ChildMap, range: &Range<LAddr>, mut func: F) -> Option<R>
where
    F: FnMut(Range<LAddr>) -> Option<R>,
{
    let mut start = range.start;
    for (&base, child) in map {
        if start < base {
            if let Some(ret) = func(start..base) {
                return Some(ret);
            }
        }
        start = child.end(base);
    }
    if start < range.end {
        func(start..range.end)
    } else {
        None
    }
}
