use alloc::{boxed::Box, collections::BTreeMap, format, string::String, vec::Vec};
use core::{array, iter, mem};

use solvent::{
    impl_obj_for,
    prelude::{Handle, Object, Packet},
};

use crate::Error;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Header {
    pub magic: usize,
    pub method_id: usize,
    pub metadata_count: usize,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Metadata {
    pub is_handle: bool,
    pub len: usize,
}

pub struct Serializer<'a>(&'a mut Packet);

impl<'a> Serializer<'a> {
    #[inline]
    fn extend_from_slice(&mut self, slice: &[u8]) {
        self.0.buffer.extend_from_slice(slice);
    }
}

impl Extend<u8> for Serializer<'_> {
    #[inline]
    fn extend<T: IntoIterator<Item = u8>>(&mut self, iter: T) {
        self.0.buffer.extend(iter)
    }
}

impl Extend<Handle> for Serializer<'_> {
    #[inline]
    fn extend<T: IntoIterator<Item = Handle>>(&mut self, iter: T) {
        self.0.handles.extend(iter)
    }
}

pub struct Deserializer<'a> {
    buffer: &'a [u8],
    handles: &'a [Handle],
}

impl<'a> Deserializer<'a> {
    fn check_buffer(&self, len: usize) -> Result<(), Error> {
        if self.buffer.len() > len {
            Ok(())
        } else {
            Err(Error::BufferTooShort {
                len: self.buffer.len(),
                expected_at_least: len,
            })
        }
    }

    fn check_handles(&self, len: usize) -> Result<(), Error> {
        if self.handles.len() > len {
            Ok(())
        } else {
            Err(Error::BufferTooShort {
                len: self.handles.len(),
                expected_at_least: len,
            })
        }
    }

    fn next_buffer(&mut self, len: usize) -> Result<&[u8], Error> {
        self.check_buffer(len)?;
        let (ret, next) = self.buffer.split_at(len);
        self.buffer = next;
        Ok(ret)
    }

    fn next_handle(&mut self) -> Result<Handle, Error> {
        self.check_handles(1)?;
        let (&ret, next) = self.handles.split_first().unwrap();
        self.handles = next;
        Ok(ret)
    }
}

pub trait SerdePacket: Sized {
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error>;

    fn deserialize(de: &mut Deserializer) -> Result<Self, Error>;
}

impl SerdePacket for () {
    #[inline]
    fn serialize(self, _: &mut Serializer) -> Result<(), Error> {
        Ok(())
    }

    #[inline]
    fn deserialize(_: &mut Deserializer) -> Result<Self, Error> {
        Ok(())
    }
}

impl SerdePacket for bool {
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        ser.extend_one(self as u8);
        Ok(())
    }

    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        let byte = de.next_buffer(1)?;
        Ok(byte[0] != 0)
    }
}

macro_rules! serde_basic {
    ($ty:ident) => {
        impl SerdePacket for $ty {
            fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
                ser.extend_from_slice(&self.to_ne_bytes());
                Ok(())
            }

            fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
                let bytes = de.next_buffer(mem::size_of::<Self>())?;
                Ok(Self::from_ne_bytes(bytes.try_into().unwrap()))
            }
        }
    };
    ($($ty:ident),* $(,)?) => {
        $(serde_basic!($ty);)*
    }
}
serde_basic!(u8, u16, u32, usize, u64, u128, i8, i16, i32, isize, i64, i128, f32, f64);

impl<T: SerdePacket, const N: usize> SerdePacket for [T; N] {
    #[inline]
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        self.into_iter().try_for_each(|elem| elem.serialize(ser))
    }

    #[inline]
    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        array::try_from_fn(|_| T::deserialize(de))
    }
}

macro_rules! serde_tuples {
    (@INNER $($ty:ident),+ $(,)?) => {
        #[allow(non_snake_case)]
        impl<$($ty : SerdePacket),+> SerdePacket for ($($ty,)+) {
            fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
                let ($($ty,)+) = self;
                $($ty.serialize(ser)?;)+
                Ok(())
            }

            fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
                $(let $ty = <$ty>::deserialize(de)?;)+
                Ok(($($ty,)+))
            }
        }
    };
    () => {};
    ($head:ident, $($ty:ident),* $(,)?) => {
        serde_tuples!(@INNER $head, $($ty),*);
        serde_tuples!($($ty,)*);
    };
}
serde_tuples!(A, B, C, D, E, F, G, H, I, J, K, L);

impl<T: SerdePacket> SerdePacket for Box<T> {
    #[inline]
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        Box::into_inner(self).serialize(ser)
    }

    #[inline]
    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        T::deserialize(de).map(Box::new)
    }
}

impl<T: SerdePacket> SerdePacket for Vec<T> {
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        self.len().serialize(ser)?;
        self.into_iter().try_for_each(|elem| elem.serialize(ser))
    }

    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        let len = usize::deserialize(de)?;
        iter::repeat_with(|| T::deserialize(de))
            .take(len)
            .try_collect()
    }
}

impl SerdePacket for String {
    #[inline]
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        self.into_bytes().serialize(ser)
    }

    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        let bytes = Vec::<u8>::deserialize(de)?;
        String::from_utf8(bytes).map_err(|err| Error::TypeMismatch(err.into()))
    }
}

impl<K: Ord + SerdePacket, V: SerdePacket> SerdePacket for BTreeMap<K, V> {
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        self.len().serialize(ser)?;
        self.into_iter().try_for_each(|kv| kv.serialize(ser))
    }

    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        let len = usize::deserialize(de)?;
        iter::repeat_with(|| <(K, V)>::deserialize(de))
            .take(len)
            .try_collect()
    }
}

impl SerdePacket for solvent::error::Error {
    #[inline]
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        self.into_retval().serialize(ser)
    }

    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        let retval = usize::deserialize(de)?;
        Self::try_from_retval(retval)
            .ok_or_else(|| Error::TypeMismatch("unknown error type".into()))
    }
}

impl SerdePacket for Handle {
    #[inline]
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        ser.extend_one(self);
        Ok(())
    }

    #[inline]
    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        de.next_handle()
    }
}

impl SerdePacket for Option<Handle> {
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        let handle = self.unwrap_or(Handle::NULL);
        handle.serialize(ser)
    }

    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        let handle = Handle::deserialize(de)?;
        Ok(handle.check_null().ok())
    }
}

macro_rules! serde_ko {
    ($ty:ty) => {
        impl SerdePacket for $ty {
            fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
                Self::ID.serialize(ser)?;
                ser.extend_one(Self::into_raw(self));
                Ok(())
            }

            fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
                let id = usize::deserialize(de)?;
                if id != Self::ID {
                    return Err(Error::TypeMismatch(
                        format!("expected {} ({}), found {id}", Self::NAME, Self::ID).into(),
                    ));
                }
                let handle = de.next_handle()?;
                Ok(unsafe { Self::from_raw(handle) })
            }
        }

        impl SerdePacket for Option<$ty> {
            fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
                self.map(<$ty>::into_raw).serialize(ser)
            }

            fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
                let id = usize::deserialize(de)?;
                if id != <$ty>::ID {
                    return Err(Error::TypeMismatch(
                        format!("expected {} ({}), found {id}", <$ty>::NAME, <$ty>::ID).into(),
                    ));
                }
                let handle = Option::<Handle>::deserialize(de)?;
                Ok(handle.map(|handle| unsafe { <$ty>::from_raw(handle) }))
            }
        }
    };
}
impl_obj_for!(serde_ko);
