use core::ops::Range;

use sv_call::res::*;

use crate::{error::Result, obj::Object};
trait ResData: Copy {
    fn to_usize(self) -> usize;
}

macro_rules! impl_res_data {
    ($($name:ident),*) => {
        $(impl ResData for $name {
            #[inline]
            fn to_usize(self) -> usize {
                self as usize
            }
        })*
    }
}
impl_res_data!(usize, u16, u32);

trait Resource: Sized + Object {
    type Data: ResData;
    const TYPE: u32;

    fn allocate(&self, range: Range<Self::Data>) -> Result<Self> {
        let base = range.start.to_usize();
        let size = range.end.to_usize() - base;
        let child =
            sv_call::sv_res_alloc(unsafe { self.raw() }, Self::TYPE, base, size).into_res()?;
        Ok(unsafe { Self::from_raw(child) })
    }
}

macro_rules! impl_resource {
    ($name:ident, $data:ident, $type:ident) => {
        #[repr(transparent)]
        pub struct $name(sv_call::Handle);
        crate::impl_obj!($name);
        crate::impl_obj!(@DROP, $name);

        impl Resource for $name {
            type Data = $data;

            const TYPE: u32 = $type;
        }

        impl $name {
            #[inline]
            pub fn allocate(&self, range: Range<$data>) -> Result<Self> {
                Resource::allocate(self, range)
            }
        }
    };
}

impl_resource!(MemRes, usize, RES_MEM);
impl_resource!(PioRes, u16, RES_PIO);
impl_resource!(GsiRes, u32, RES_GSI);
