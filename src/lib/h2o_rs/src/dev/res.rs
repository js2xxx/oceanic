use core::ops::Range;

use sv_call::{res::*, SV_INTRRES, SV_MEMRES, SV_PIORES};

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
        let child = unsafe {
            sv_call::sv_res_alloc(unsafe { self.raw() }, Self::TYPE, base, size).into_res()?
        };
        Ok(unsafe { Self::from_raw(child) })
    }
}

macro_rules! impl_resource {
    ($name:ident, $data:ident, $type:ident, $num:ident) => {
        #[repr(transparent)]
        #[derive(Debug)]
        pub struct $name(sv_call::Handle);
        crate::impl_obj!($name, $num);
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

impl_resource!(MemRes, usize, RES_MEM, SV_MEMRES);
impl_resource!(PioRes, u16, RES_PIO, SV_PIORES);

#[repr(transparent)]
#[derive(Debug)]
pub struct IntrRes(sv_call::Handle);
crate::impl_obj!(IntrRes, SV_INTRRES);
crate::impl_obj!(@CLONE, IntrRes);
crate::impl_obj!(@DROP, IntrRes);
