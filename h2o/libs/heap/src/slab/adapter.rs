use core::{marker::PhantomData, ptr::NonNull};

use intrusive_collections::{Adapter, DefaultLinkOps, KeyAdapter, RBTreeLink};

pub struct PagePointerOps(PhantomData<super::Page>);

unsafe impl intrusive_collections::PointerOps for PagePointerOps {
    type Value = super::Page;

    type Pointer = NonNull<super::Page>;

    unsafe fn from_raw(&self, value: *const Self::Value) -> Self::Pointer {
        NonNull::new_unchecked(value as *mut _)
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_raw(&self, ptr: Self::Pointer) -> *const Self::Value {
        ptr.as_ptr()
    }
}

pub struct PageAdapter {
    link_ops: <RBTreeLink as DefaultLinkOps>::Ops,
    pointer_ops: PagePointerOps,
}

impl PageAdapter {
    pub const NEW: Self = PageAdapter {
        link_ops: <RBTreeLink as DefaultLinkOps>::NEW,
        pointer_ops: PagePointerOps(PhantomData),
    };
}

impl Default for PageAdapter {
    fn default() -> Self {
        Self::NEW
    }
}

// unsafe impl Send for PageAdapter {}
unsafe impl Sync for PageAdapter {}

unsafe impl Adapter for PageAdapter {
    type LinkOps = <RBTreeLink as DefaultLinkOps>::Ops;

    type PointerOps = PagePointerOps;

    unsafe fn get_value(
        &self,
        link: <Self::LinkOps as intrusive_collections::LinkOps>::LinkPtr,
    ) -> *const <Self::PointerOps as intrusive_collections::PointerOps>::Value {
        intrusive_collections::container_of!(link.as_ptr(), super::Page, link)
    }

    unsafe fn get_link(
        &self,
        value: *const <Self::PointerOps as intrusive_collections::PointerOps>::Value,
    ) -> <Self::LinkOps as intrusive_collections::LinkOps>::LinkPtr {
        // We need to do this instead of just accessing the field directly
        // to strictly follow the stack borrow rules.
        let ptr = (value.cast::<u8>()).add(intrusive_collections::offset_of!(super::Page, link));
        core::ptr::NonNull::new_unchecked(ptr as *mut _)
    }

    fn link_ops(&self) -> &Self::LinkOps {
        &self.link_ops
    }

    fn link_ops_mut(&mut self) -> &mut Self::LinkOps {
        &mut self.link_ops
    }

    fn pointer_ops(&self) -> &Self::PointerOps {
        &self.pointer_ops
    }
}

impl<'a> KeyAdapter<'a> for PageAdapter {
    type Key = usize;
    fn get_key(&self, page: &'a super::Page) -> Self::Key {
        page.free_count()
    }
}
