use core::arch::asm;

use crate::*;

fn create_table(
    entry: &mut Entry,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<NonNull<Table>, Error> {
    log::trace!(
            "paging::create_table: entry = {:?}(value = {:?}), level = {:?}, id_off = {:?}, allocator = {:?}",
            entry as *mut _,
            *entry,
            level, id_off,
            allocator as *mut _
      );

    assert!(level != Level::Pt, "Too low level");

    match entry.get_table(id_off, level) {
        Some(ptr) => Ok(ptr),
        None => {
            if entry.is_leaf(level) {
                Err(Error::EntryExistent(true))
            } else {
                let phys =
                    unsafe { allocator.allocate_zeroed(id_off) }.ok_or(Error::OutOfMemory)?;
                let attr = Attr::INTERMEDIATE;
                *entry = Entry::new(phys, attr, Level::Pt);
                log::trace!(
                    "paging::create_table: allocated new table at phys {:?}",
                    phys
                );
                Ok(entry
                    .get_table(id_off, level)
                    .expect("Failed to get the table of the entry"))
            }
        }
    }
}

fn split_table(
    entry: &mut Entry,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
    let (phys, mut attr) = entry.get(Level::Pt);
    entry.reset();
    attr &= !Attr::LARGE_PAT;

    let item_level = level.decrease().expect("Too low level");
    let item_attr = item_level.leaf_attr(attr);
    let mut table = create_table(entry, level, id_off, allocator)?;
    let table_mut = unsafe { table.as_mut() };
    for (i, item) in table_mut.iter_mut().enumerate() {
        let item_phys = PAddr::new(*phys + i * item_level.page_size());
        *item = Entry::new(item_phys, item_attr, item_level);
    }

    Ok(())
}

fn get_or_split_table(
    entry: &mut Entry,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<NonNull<Table>, Error> {
    assert!(level != Level::Pt, "Too low level");

    entry.get_table(id_off, level).map_or_else(
        || {
            if entry.is_leaf(level) {
                split_table(entry, level, id_off, allocator)?;

                Ok(entry
                    .get_table(id_off, level)
                    .expect("Failed to get the table of the entry"))
            } else {
                Err(Error::EntryExistent(false))
            }
        },
        Ok,
    )
}

fn destroy_tables<A>(tables: A, id_off: usize, allocator: &mut impl PageAlloc)
where
    A: IntoIterator<Item = Option<(NonNull<Entry>, Level)>>,
{
    for (mut item, elvl) in tables.into_iter().flatten().fuse() {
        let lvl = elvl.increase().unwrap();
        unsafe {
            let item = item.as_mut();
            let table = item.get_table(id_off, lvl).unwrap();
            if table.as_ref().is_empty(None, elvl) {
                let (addr, _) = item.get(Level::Pt);
                item.reset();
                allocator.deallocate(addr);
            } else {
                break;
            }
        }
    }
}

pub(crate) unsafe fn invalidate_page(virt: LAddr) {
    asm!("invlpg [{}]", in(reg) *virt);
}

pub(crate) fn new_page(
    root_table: &mut Table,
    virt: LAddr,
    phys: PAddr,
    attr: Attr,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
    log::trace!(
        "paging::new_page: root table = {:?}, virt = {:?}, phys = {:?}, attr = {:?}, level = {:?}, id_off = {:?}, allocator = {:?}",
        root_table as *mut _,
        virt,
        phys,
        attr,
        level,
        id_off,
        allocator as *mut _
    );

    let mut table: NonNull<Table> = NonNull::from(root_table);
    let mut lvl = Level::P4;
    loop {
        let item = unsafe { &mut table.as_mut()[lvl.addr_idx(virt, false)] };

        if lvl == level {
            break if item.is_leaf(level) {
                Err(Error::EntryExistent(true))
            } else {
                let attr = level.leaf_attr(attr);
                *item = Entry::new(phys, attr, level);

                unsafe { invalidate_page(virt) };
                Ok(())
            };
        }

        table = create_table(item, lvl, id_off, allocator)?;
        lvl = lvl.decrease().expect("Too low level");
    }
}

pub(crate) fn modify_page(
    root_table: &mut Table,
    virt: LAddr,
    attr: Attr,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
    let mut table: NonNull<Table> = NonNull::from(root_table);
    let mut lvl = Level::P4;
    loop {
        let item = unsafe { &mut table.as_mut()[lvl.addr_idx(virt, false)] };

        if lvl == level {
            break if item.is_leaf(level) {
                let attr = level.leaf_attr(attr);
                let (phys, _) = item.get(level);
                *item = Entry::new(phys, attr, level);

                unsafe { invalidate_page(virt) };
                Ok(())
            } else {
                Err(Error::EntryExistent(false))
            };
        }

        table = get_or_split_table(item, lvl, id_off, allocator)?;
        lvl = lvl.decrease().expect("Too low level");
    }
}

pub(crate) fn get_page(
    root_table: &Table,
    virt: LAddr,
    id_off: usize,
) -> Result<(PAddr, Attr), Error> {
    let mut table: NonNull<Table> = NonNull::from(root_table);
    let mut lvl = Level::P4;
    loop {
        let item = unsafe { &table.as_ref()[lvl.addr_idx(virt, false)] };

        if item.is_leaf(lvl) {
            let offset = virt.val() & !lvl.addr_mask() as usize;
            let (base, attr) = item.get(lvl);
            break Ok((PAddr::new(*base | offset), attr));
        }

        table = item
            .get_table(id_off, lvl)
            .ok_or(Error::EntryExistent(false))?;
        lvl = lvl.decrease().ok_or(Error::EntryExistent(false))?;
    }
}

pub(crate) fn drop_page(
    root_table: &mut Table,
    virt: LAddr,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
    let mut table: NonNull<Table> = NonNull::from(root_table);
    let mut lvl = Level::P4;

    let mut parent = None;
    // Contains page tables that have only one entry which may be dropped, and its
    // entry's level.
    let mut empty_tables = [None::<(NonNull<Entry>, Level)>; 5];

    loop {
        let item = {
            let idx = lvl.addr_idx(virt, false);
            let table_mut = unsafe { table.as_mut() };
            if table_mut.is_empty(Some(idx), lvl) {
                empty_tables[lvl as usize] = parent.map(|p| (NonNull::from(p), lvl));
            }
            &mut table_mut[idx]
        };

        if lvl == level {
            break if item.is_leaf(level) {
                item.reset();

                unsafe { invalidate_page(virt) };
                destroy_tables(empty_tables, id_off, allocator);
                Ok(())
            } else {
                Err(Error::EntryExistent(false))
            };
        }

        table = get_or_split_table(item, lvl, id_off, allocator)?;
        lvl = lvl.decrease().expect("Too low level");
        parent = Some(item);
    }
}

pub(crate) fn check(virt: &Range<LAddr>, phys: Option<PAddr>) -> Result<(), Error> {
    log::trace!("paging::check: virt = {:?}, phys = {:?}", virt, phys);

    #[inline]
    fn misaligned<Origin>(addr: usize, o: Origin) -> Option<Origin> {
        if addr & PAGE_MASK == 0 {
            None
        } else {
            log::warn!("paging::check: misaligned address: {:#x}", addr);
            Some(o)
        }
    }

    let (vstart, vend) = (virt.start.val(), virt.end.val());
    let ret = Error::AddrMisaligned {
        vstart: misaligned(vstart, virt.start),
        vend: misaligned(vend, virt.end),
        phys: phys.and_then(|phys| misaligned(*phys, phys)),
    };
    if !ret.is_misaligned_invalid() {
        return Err(ret);
    }

    if vstart >= vend {
        log::warn!("paging::check: linear address range is empty");
        return Err(Error::RangeEmpty);
    }

    Ok(())
}
