pub mod space;

use paging::LAddr;

use core::ptr::NonNull;

#[inline(never)]
unsafe fn alloc_pages(n: usize) -> Option<NonNull<[heap::Page]>> {
      let laddr = pmm::alloc_pages_exact(n, None)?.to_laddr(minfo::ID_OFFSET);
      let ptr = NonNull::new(laddr.cast::<heap::Page>());
      ptr.map(|ptr| NonNull::slice_from_raw_parts(ptr, n))
}

#[inline(never)]
unsafe fn dealloc_pages(pages: NonNull<[heap::Page]>) {
      let paddr = LAddr::new(pages.as_ptr().cast()).to_paddr(minfo::ID_OFFSET);
      let n = pages.len();
      pmm::dealloc_pages_exact(n, paddr);
}

/// Initialize the PMM and the kernel heap (Rust global allocator).
pub fn init() {
      let all_available = pmm::init(
            crate::KARGS.efi_mmap_paddr,
            crate::KARGS.efi_mmap_len,
            crate::KARGS.efi_mmap_unit,
            minfo::TRAMPOLINE_RANGE,
      );
      log::info!(
            "Memory size: {:.3} GB ({:#x} Bytes)",
            (all_available as f64) / 1073741824.0,
            all_available
      );
      unsafe { heap::set_alloc(alloc_pages, dealloc_pages) };
      heap::test(archop::rand::get() as usize);
}

mod syscall {
      use bitop_ex::BitOpEx;
      use solvent::*;

      #[syscall]
      fn alloc_pages(virt: *mut u8, phys: usize, size: usize, align: usize, flags: u32) -> *mut u8 {
            use super::space;

            if size.contains_bit(paging::PAGE_MASK) || !align.is_power_of_two() {
                  return Err(Error(EINVAL));
            }

            let ty = if virt.is_null() {
                  space::AllocType::Layout(
                        core::alloc::Layout::from_size_align(size, align)
                              .map_err(|_| Error(EINVAL))?,
                  )
            } else {
                  // TODO: Check whether the virtual address is permitted.
                  space::AllocType::Virt(
                        paging::LAddr::new(virt)..paging::LAddr::new(unsafe { virt.add(size) }),
                  )
            };

            // TODO: Check whether the physical address is permitted.
            let phys = (phys != 0).then_some(paging::PAddr::new(phys));

            let flags = space::Flags::from_bits(flags).ok_or(Error(EINVAL))?;

            let ret = {
                  let _sched = crate::sched::SCHED.lock();
                  space::current().alloc(ty, phys, flags)
            };
            ret.map_err(Into::into).map(|mut b| b.as_mut_ptr())
      }

      #[syscall]
      fn dealloc_pages(ptr: *mut u8, size: usize) {
            use super::space;

            if size.contains_bit(paging::PAGE_MASK) {
                  return Err(Error(EINVAL));
            }

            let ret = unsafe {
                  let b = core::pin::Pin::new_unchecked(core::slice::from_raw_parts_mut(ptr, size));
                  let _sched = crate::sched::SCHED.lock();
                  space::current().dealloc(b)
            };
            ret.map_err(Into::into)
      }

      #[syscall]
      fn modify_pages(ptr: *mut u8, size: usize, flags: u32) {
            use super::space;

            if size.contains_bit(paging::PAGE_MASK) {
                  return Err(Error(EINVAL));
            }
            let flags = space::Flags::from_bits(flags).ok_or(Error(EINVAL))?;

            let ret = unsafe {
                  let b = core::pin::Pin::new_unchecked(core::slice::from_raw_parts_mut(ptr, size));
                  let _sched = crate::sched::SCHED.lock();
                  space::current().modify(b, flags)
            };
            ret.map_err(Into::into)?;
            Ok(())
      }
}
