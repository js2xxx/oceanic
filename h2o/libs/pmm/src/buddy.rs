//! # Physical memory manager implementations
//!
//! The H2O's PMM is based on Linux's buddy allocation system. Likewise, it has 2
//! allocation zones as symboled as [`PFType`] - the lower, for the
//! space below 4GB, and the higher, for the space above 4GB. It just simplifies the
//! method, and for future extensions, can be extended easily.
//!
//! It has 2 interface pairs, one for rough pow(2, k) page(s) allocations as at
//! [`alloc_pages`] and [`dealloc_pages`],
//! and another for exact n page(s) allocations as at
//! [`alloc_pages_exact`] and
//! [`dealloc_pages_exact`].
//!
//! ## Buddy Allocation System
//!
//! The buddy allocation system (abbr. BAS) is designed for fast and continuous
//! physical page frame allocations. Every allocation zone has a set of free lists
//! corresponding to page orders. Every free list holds a series of page frames, whose
//! real memory position is fixed above `KMEM_PHYS_BASE`.
//!
//! The definition of page order is at [`MAX_ORDER`].
//!
//! ### Allocating `pow(2, k)` pages
//!
//! Obviously, k is the requested page order. We than find a suitable page frame from
//! free lists ranged `k..MAX_ORDER`. If the returned
//! page order is greater the the requested, the page frame will be divided to pieces
//! sized `pow(2, k_1)`. The piece of requested size will be returned, and the
//! remaining will be saved back.
//!
//! ### Deallocating `pow(2, k)` pages
//!
//! We directly add the page to the free list of the corresponding page order - k. Than
//! We look for the buddy of the page frame. If it's also free, than we merge the 2
//! together, and pop it from the current free list and push it into the next one. The
//! operations will be done repeatedly until the page order reaches
//! [`MAX_ORDER`] or it has no buddy.
//!
//! NOTE: The specific process is at source code.
//!
//! ### (De) Allocating pages with exact sizes
//!
//! See [`alloc_pages_exact`] for more.

use super::KMEM_PHYS_BASE;
use super::{PAddr, PAGE_SHIFT, PAGE_SIZE};
use bitop_ex::BitOpEx;

use core::cell::Cell;
use core::cmp::min;
use core::mem::size_of;
use core::ops::Range;
use core::ptr::NonNull;
use intrusive_collections::intrusive_adapter;
use intrusive_collections::{LinkedList, LinkedListLink};
use spin::Mutex;

/// The boundary for [`PFType`].
///
/// Below the boundary belongs to [`PFType::Low`]; above to [`PFType::High`].
const PFTYPE_BOUND: usize = 0x1_0000_0000;

/// The max page order for allocation.
///
/// Page orders describe the size of pages. A page order of `k` represents a page sized
/// `pow(2, k)`. Pages are divided in such way to simplify allocations.
pub const MAX_ORDER: usize = 24 - PAGE_SHIFT;

/// The [`Range`] of all available page orders.
pub const ORDERS: Range<usize> = 0..MAX_ORDER;

/// The number of all available page orders.
pub const NR_ORDERS: usize = MAX_ORDER;

/// The size of page frame list (a.k.a. free list).
const PF_LIST_SIZE: usize = size_of::<PFList>();

/// The size of all the page frame lists.
const PF_LISTS_SIZE: usize = PF_LIST_SIZE * (PFType::Max as usize) * NR_ORDERS;

/// The spinlock for the PMM.
///
/// The PMM is single-cpued, so only one cpu / thread can access PMM at one time.
static PMM_LOCK: Mutex<()> = Mutex::new(());

/// The page frame structure.
///
/// A page frame structure represents a physical page sized variedly, which can only
/// be stored above [`KMEM_PHYS_BASE`] statically.
#[repr(C)]
struct PageFrame {
      /// The link to the free list.
      link: LinkedListLink,

      /// The order of the page.
      ///
      /// A page's order cannot be easily deduced by searching free lists, so we
      /// create a field to store the info for easier access.
      order: Cell<usize>,
}
pub const PF_SIZE: usize = core::mem::size_of::<PageFrame>();

intrusive_adapter!(PFAdapter = &'static PageFrame: PageFrame { link: LinkedListLink });

/// The free list type.
type PFList = LinkedList<PFAdapter>;

/// The page frame type for allocation. See [the module level doc](./index.html) for
/// more.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PFType {
      /// Representing the low (below 4GB) physical memory area.
      Low,
      /// Representing the high (above 4GB) physical memory area.
      High,
      /// Representing both types above in functions, as well as the number of types.
      Max,
}

/// The kernel data containing all the free lists.
///
/// We cannot declare free lists directly because [`Cell`] is used in the type
/// declaration, which makes it unable to be declared as static variable for
/// multi-threaded systems. So we do it indirectly and create a function for
/// access.
static mut PFL_DATA: [u8; PF_LISTS_SIZE] = [0; PF_LISTS_SIZE];

impl From<PAddr> for PFType {
      /// Get a page frame type of a physical address.
      ///
      /// If the address is under 0x1_0000_0000, a.k.a. [`PFTYPE_BOUND`], the type is
      /// low, otherwise it is high.
      #[inline]
      fn from(addr: PAddr) -> PFType {
            if (0..PFTYPE_BOUND).contains(&addr) {
                  PFType::Low
            } else {
                  PFType::High
            }
      }
}

/// The page frame base for different [`PFType`]s
///
/// The base of `PFType::Low` is at 0, while the base of `PFType::High` is at
/// [`PFTYPE_BOUND`].
///
/// # Arguments
///
/// * `pftype` - The requested `PFType`
///
/// # Panics
///
/// If `pftype` is [`PFType::Max`], it will panic.
#[inline]
fn pft_base(pftype: PFType) -> *const PageFrame {
      unsafe {
            page_frame(PAddr::new(match pftype {
                  PFType::Low => 0usize,
                  PFType::High => PFTYPE_BOUND,
                  PFType::Max => panic!("Invalid PFType"),
            })) as *const PageFrame
      }
}

/// Convert a physical address to its corresponding page frame struct.
///
/// # Examples
/// ```
/// let page = page_frame(0 as PAddr);
/// assert_eq!(page as *const PageFrame, KMEM_PHYS_BASE as *const PageFrame);
/// ```
///
/// # Safety
///
/// It will be always safe **UNLESS** the `addr` is out of all the usable physical
/// memory range.
#[inline]
unsafe fn page_frame(addr: PAddr) -> &'static PageFrame {
      (KMEM_PHYS_BASE as *const PageFrame)
            .add(*addr >> PAGE_SHIFT)
            .as_ref()
            .unwrap()
}

// #[inline]
// unsafe fn page_frame_mut(addr: PAddr) -> &'static mut PageFrame {
//       (KMEM_PHYS_BASE as *mut PageFrame)
//             .add(addr >> PAGE_SHIFT)
//             .as_mut()
//             .unwrap()
// }

/// Convert a page frame struct to its corresponding physical address.
///
/// This is the inverse function of [`page_frame`].
///
/// # Examples
/// ```
/// let page = page_frame(0 as PAddr);
/// let addr = page_address(page);
/// assert_eq!(addr, 0 as PAddr);
/// ```
///
/// # Safety
///
/// It'll always be safe **UNLESS** the requested [`PageFrame`] is invalid (e.g.
/// as generated by an invalid call from [`page_frame`]).
#[inline]
unsafe fn page_address(page: &PageFrame) -> PAddr {
      PAddr::new(
            ((page as *const PageFrame).offset_from(KMEM_PHYS_BASE as *const PageFrame)
                  << PAGE_SHIFT) as usize,
      )
}

/// Convert a page frame to its corresponding PFN.
///
/// A PFN is the number of a page frame in a allocation zone marked by [`PFType`].
/// It is the offset between the requested page frame and what is described at
/// [`pft_base`]. We use PFN to calculate the buddies of page frames.
///
/// # Arguments
///
/// * `page` - The requested page frame
/// * `pftype` - The `PFType` of which `page` is.
///
/// # Examples
/// ```
/// let page = page_frame(0 as PAddr);
/// let pftype = PFType::Low;
/// let pfn = page_pfn(page, pftype);
/// assert_eq!(pfn, 0usize);
///
/// let page = page_frame(PFTYPE_BOUND as PAddr);
/// let pftype = PFType::High;
/// let pfn = page_pfn(page, pftype);
/// assert_eq!(pfn, 0usize);
/// ```
///
/// # Safety
///
/// It'll always be safe **UNLESS** the requested [`PageFrame`] is invalid (e.g.
/// as generated by an invalid call from [`page_frame`]).
#[inline]
unsafe fn page_to_pfn(page: &PageFrame, pftype: PFType) -> usize {
      let base = pft_base(pftype);

      (page as *const PageFrame).offset_from(base) as usize
}

/// Convert a PFN to its corresponding page frame
///
/// The declaration of PFN is at [`page_to_pfn`].
///
/// # Arguments
///
/// * `pfn` - The requested PFN.
/// * `pftype` - The `PFType` of which `page` is.
///
/// # Examples
///
/// The examples is at [`page_to_pfn`].
///
/// # Safety
///
/// It'll always be safe **UNLESS** the requested PFN is invalid (e.g.
/// as randomly generated).
#[inline]
unsafe fn pfn_to_page(pfn: usize, pftype: PFType) -> &'static PageFrame {
      let base = pft_base(pftype);

      &*base.add(pfn)
}

/// The static [`PFList`] instance
///
/// Request a specific free list with a definite [`PFType`] and a page order.
/// See [the module level doc](./index.html) for more.
///
/// # Errors
///
/// It'll return a `None` value only if the requested `PFType` is invalid or
/// the page order is out of range.
#[inline]
fn pf_list(pftype: PFType, order: usize) -> Option<&'static PFList> {
      if ORDERS.contains(&order) && pftype != PFType::Max {
            unsafe {
                  (PFL_DATA.as_ptr() as *const PFList)
                        .add(order + (pftype as usize) * NR_ORDERS)
                        .as_ref()
            }
      } else {
            None
      }
}

/// The static and mutable [`PFList`] instance
///
/// Request a specific free list with a definite [`PFType`] and a page order.
/// See [the module level doc](./index.html) for more.
///
/// # Errors
///
/// It'll return a `None` value only if the requested `PFType` is invalid or
/// the page order is out of range.
#[inline]
fn pf_list_mut(pftype: PFType, order: usize) -> Option<&'static mut PFList> {
      if ORDERS.contains(&order) && pftype != PFType::Max {
            unsafe {
                  (PFL_DATA.as_ptr() as *mut PFList)
                        .add(order + (pftype as usize) * NR_ORDERS)
                        .as_mut()
            }
      } else {
            None
      }
}

/// Split a large page into smaller ones.
///
/// When responding with a page larger than requested, we need to split the page
/// into smaller ones to make the best use of memory. It do things like this:
///
///     Before: |<-----------------a large page---------------->|
///     After:  |<--->|<--->|<--------->|<--------------------->|
///              ^used  ^-----------left unused------------^
///
/// # Arguments
///
/// * `page` - The requested page frame
/// * `pftype` - The [`PFType`] of the page frame
/// * `orders` - The [`Range`] that the spliting operates within
///
/// # Panics
///
/// If `pftype` is [`PFType::Max`] or `orders` is invalid, it will panic.
///
/// # Returns
///
/// It returns the page address that is requested to be split.
fn split_page(page: &PageFrame, pftype: PFType, orders: Range<usize>) -> &PageFrame {
      assert_ne!(pftype, PFType::Max, "Invalid PFType");
      assert!(orders.end < ORDERS.end, "Invalid page order range");

      for o in orders {
            let sib = unsafe { &*(page as *const PageFrame).add(1 << o) };
            let pflist = pf_list_mut(pftype, o).unwrap();

            sib.order.set(o + 1);
            if sib.link.is_linked() {
                  unsafe { sib.link.force_unlink() };
            }
            pflist.push_back(sib);
      }

      page
}

/// Allocate a page with a specific page order and a [`PFType`].
///
/// The function is locked in the whole context so as to protect its atomicity.
/// NOTE: The PMM is a single-cpued / single-threaded module.
///
/// See [the module level doc](./index.html) for more.
///
/// # Errors
///
/// It returns a `None` value **ONLY** if there's no more such page for
/// allocation (a.k.a. memory exhausted).
///
/// # Panics
///
/// If `order` is out of available range or `pftype` is [`PFType::Max`], it will
/// panic.
fn alloc_page_typed(order: usize, pftype: PFType) -> Option<PAddr> {
      assert_ne!(pftype, PFType::Max, "Invalid PFType");
      assert!(ORDERS.contains(&order), "Invalid page order");

      let _lock = PMM_LOCK.lock();

      for o in order..MAX_ORDER {
            let pflist = pf_list_mut(pftype, o).unwrap();

            if let Some(page) = pflist.pop_front() {
                  page.order.set(0);
                  let page = split_page(page, pftype, order..o);
                  return Some(unsafe { page_address(page) });
            }
      }

      None
}

/// Get the buddy page frame for a specific page.
///
/// The buddy algorithm uses PFNs as described at [`page_to_pfn`]. It subtly uses
/// bit-xor operator to jump from one page to another, its buddy page.
///
///               <____________________>
///              /                      \
///     |<------a page------->|<-----its buddy----->|
///     or      |                        |
///     |<-----its buddy----->|<------a page------->|
///
/// The specific page order determines which level its buddy page is in.
///
/// # Panics
///
/// If `order` is out of available range or `pftype` is [`PFType::Max`], it will
/// panic.
///
/// # Safety
///
/// It'll always be safe **UNLESS** the requested page frame is invalid.
#[inline]
unsafe fn get_buddy(page: &PageFrame, order: usize, pftype: PFType) -> &'static PageFrame {
      assert_ne!(pftype, PFType::Max, "Invalid PFType");
      assert!(ORDERS.contains(&order), "Invalid page order");

      let pfn = page_to_pfn(page, pftype);
      let buddy = pfn ^ (1 << order);
      pfn_to_page(buddy, pftype)
}

/// Get the combined page frame of a page and its buddy.
///
/// The combined page is the page of a lower address of a page and its buddy page.
///
/// See [`get_buddy`] for more.
///
/// # Panics
///
/// If `order` is out of available range or `pftype` is [`PFType::Max`], it will
/// panic.
///
/// # Safety
///
/// It'll always be safe **UNLESS** the requested page frame is invalid.
#[inline]
unsafe fn get_combined(page: &PageFrame, order: usize, pftype: PFType) -> &'static PageFrame {
      assert_ne!(pftype, PFType::Max, "Invalid PFType");
      assert!(ORDERS.contains(&order), "Invalid page order");

      let pfn = page_to_pfn(page, pftype);
      let buddy = pfn ^ (1 << order);
      let comb = pfn & buddy;
      pfn_to_page(comb, pftype)
}

/// Deallocate a page frame with a specific page order and a [`PFType`].
///
/// The function is locked in the whole context so as to protect its atomicity.
/// Reclaim: The PMM is a single-cpued / single-threaded module.
///
/// See [the module level doc](./index.html) for more.
///
/// # Panics
///
/// If `order` is out of available range or `pftype` is [`PFType::Max`], it will
/// panic.
///
/// # Safety
///
/// It'll always be safe **UNLESS** the requested page frame is invalid.
#[allow(unused_unsafe)]
unsafe fn dealloc_page_typed(page: &'static PageFrame, order: usize, pftype: PFType) {
      assert_ne!(pftype, PFType::Max, "Invalid PFType");
      assert!(ORDERS.contains(&order), "Invalid page order");
      let _lock = PMM_LOCK.lock();

      let mut o = order;
      let mut p = page;

      // Merge the page and its buddies
      while o < (MAX_ORDER - 1) {
            let pflist = pf_list_mut(pftype, o).unwrap();
            let buddy = get_buddy(p, o, pftype);
            if buddy.order.get() != o + 1 {
                  break;
            }

            let mut bcur = pflist.cursor_mut_from_ptr(buddy as *const PageFrame);
            bcur.remove();

            p = get_combined(p, o, pftype);

            o += 1;
      }

      // Push the result into the final free list
      let pflist = pf_list_mut(pftype, o).unwrap();

      p.order.set(o + 1);
      if p.link.is_linked() {
            p.link.force_unlink();
      }
      pflist.push_back(p);
}

/// Allocate a page with a specific page order and an optional [`PFType`].
///
/// This function simply encapsulates [`alloc_page_typed`] with more convenient
/// options. See it for more.
///
/// # Errors:
///
/// If the requested page order is out of range or the physical memory is exhausted,
/// it'll return a `None` value.
pub fn alloc_pages(order: usize, pftype: Option<PFType>) -> Option<PAddr> {
      if !ORDERS.contains(&order) {
            return None;
      }
      let pftype = pftype.unwrap_or(PFType::Max);

      match pftype {
            PFType::Max => alloc_page_typed(order, PFType::High)
                  .or_else(|| alloc_page_typed(order, PFType::Low)),

            _ => alloc_page_typed(order, pftype),
      }
}

/// Deallocate a page frame with a specific page order.
///
/// This function simply encapsulates [`dealloc_page_typed`] with more convenient
/// operations. See it or more.
///
/// # Panics
///
/// If `order` is out of available range, it will panic.
///
/// # Safety
///
/// It'll always be safe **UNLESS** the requested physical address is invalid.
pub unsafe fn dealloc_pages(order: usize, addr: PAddr) {
      assert!(ORDERS.contains(&order), "Invalid page order");

      let pftype = PFType::from(addr);
      let page = page_frame(addr);
      dealloc_page_typed(page, order, pftype);
}

/// Scale back pages that is oversized.
///
/// It deallocates the remaining pages (left not used).
///
/// See [`alloc_pages_exact`] for more.
///
/// # Panics
///
/// If `order` is out of available range or `n` is too small (lower than `1 <<
/// order`), it will panic.
///
/// # Safety
///
/// It'll always be safe **UNLESS** the requested physical address is invalid.
unsafe fn scale_back_pages(addr: PAddr, order: usize, n: usize) -> PAddr {
      assert!(ORDERS.contains(&order), "Invalid page order");
      assert!((1 << order) >= n, "Invalid page number");

      let dn = (1 << order) - n;
      let rem = PAddr::new(*addr + (n << PAGE_SHIFT));

      dealloc_pages_exact(dn, rem);
      addr
}

/// Exactly allocate `n` pages with an optional [`PFType`].
///
/// A PMM with only `2^k` page allocations is not a excellent PMM, because this method
/// consumes too much memory resource, especially when allocating large-sized pages. So
/// there exists `alloc_pages_exact`.
///
/// This function allocates exactly `n` pages by allocating `2^k` pages first and
/// deallocate back the pages left unused.
///
/// See `dealloc_pages_exact` for deallocation.
///
/// # Errors:
///
/// If the requested page size is out of range or the physical memory is exhausted,
/// it'll return a `None` value.
pub fn alloc_pages_exact(n: usize, pftype: Option<PFType>) -> Option<PAddr> {
      let order = n.log2c();

      if !ORDERS.contains(&order) {
            return None;
      }

      if let Some(addr) = alloc_pages(order, pftype) {
            Some(unsafe { scale_back_pages(addr, order, n) })
      } else {
            None
      }
}

/// Exactly deallocate `n` pages.
///
/// It divide the memory block into 2^k pages with their largest size that
/// accurately fills it, and then deallocate them using `dealloc_page_typed`.
///
/// The dividing operation is like this:
///
///     Before:   0.......|<-------------a memory block--------------->|
///     After:    0.......|<------->|<------------------>|<------>|<-->|
///                         ^--2^4    ^--2^8               ^--2^4   ^--2^2
///
/// **NOTE**: `n` could be so large that its page order can exceed [`ORDERS`] safely.
///
/// # Safety
///
/// It will be always safe **UNLESS** the memory block is not inside of all the
/// usable physical memory range.
pub unsafe fn dealloc_pages_exact(n: usize, addr: PAddr) {
      let mut start = addr;
      let end = PAddr::new(*addr + (n << PAGE_SHIFT));

      while start < end {
            let pftype = PFType::from(start);
            let spage = page_frame(start);
            let epage = page_frame(end);

            let spfn = page_to_pfn(spage, pftype);
            let epfn = page_to_pfn(epage, pftype);

            let order = min(min(spfn.lsb(), MAX_ORDER - 1), (epfn - spfn).log2f());
            dealloc_page_typed(spage, order, pftype);
            start = PAddr::new(*start + (PAGE_SIZE << order));
      }
}

/// Parse the memory map acquired from H2O's boot loader.
///
/// We choose to make use of [`MMapType::BootCode`] so that the chosen areas will be
/// allocated as [`MMapType::Available`], because these areas are useless **BY FAR**.
///
/// **TODO**: To storage the info of other memory map types.
///
/// # Safety
///
/// It'll always be safe **UNLESS** `mmap_ptr` is invalid.
unsafe fn parse_mmap(mmap_ptr: NonNull<[uefi::table::boot::MemoryDescriptor]>) {
      use uefi::table::boot::MemoryType;

      fn is_available(ty: MemoryType) -> bool {
            matches!(ty, MemoryType::CONVENTIONAL | MemoryType::PERSISTENT_MEMORY)
      }

      let mmap = mmap_ptr.as_ref();

      for mdsc in mmap.iter() {
            if is_available(mdsc.ty) && mdsc.phys_start != 0 {
                  let n = mdsc.page_count as usize;
                  dealloc_pages_exact(n, PAddr::new(mdsc.phys_start as usize));
            }
      }
}

// /// Dump PMM data with specific [`PFType`] by module [`crate::outp::log`].
// #[cfg(debug_assertions)]
// pub fn dump_data(pftype: PFType) {
//       match pftype {
//             PFType::Max => {
//                   dump_data(PFType::Low);
//                   dump_data(PFType::High);
//             }
//             _ => {
//                   crate::logln!("{:?}-------------------------------------------", pftype);
//                   for i in ORDERS {
//                         crate::logln!("ORDER #{}:", i);
//                         let pflist = pf_list(pftype, i).unwrap();
//                         let mut u = 0;
//                         for j in pflist.iter() {
//                               crate::log!("{:X}\t", unsafe { page_address(j) });
//                               u += 1;
//                               if u % 8 == 0 {
//                                     crate::logln!("");
//                               }
//                         }
//                         if u % 8 != 0 {
//                               crate::logln!("");
//                         }
//                   }
//             }
//       }
// }

/// Initialize PMM module.
///
/// Unfortunately, we must initialize every free list manually, and it takes a long
/// time.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn init(mmap: NonNull<[uefi::table::boot::MemoryDescriptor]>) {
      for i in ORDERS {
            *(pf_list_mut(PFType::Low, i).unwrap()) = PFList::new(PFAdapter::new());
            *(pf_list_mut(PFType::High, i).unwrap()) = PFList::new(PFAdapter::new());
      }

      // NOTE: There we trust the `mmap` is valid.
      unsafe {
            parse_mmap(mmap);
      }
}
