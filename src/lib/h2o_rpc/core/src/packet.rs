use alloc::{boxed::Box, collections::BTreeMap, ffi::CString, format, string::String, vec::Vec};
use core::{array, iter, mem};

use solvent::{
    impl_obj_for,
    prelude::{Handle, Object, Packet},
};

use crate::Error;

pub const MAGIC: usize = 0xac84fb7c0391;

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
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty() && self.handles.is_empty()
    }

    pub fn check_buffer(&self, len: usize) -> Result<(), Error> {
        if self.buffer.len() >= len {
            Ok(())
        } else {
            Err(Error::BufferTooShort {
                found: self.buffer.len(),
                expected_at_least: len,
            })
        }
    }

    pub fn check_handles(&self, len: usize) -> Result<(), Error> {
        if self.handles.len() >= len {
            Ok(())
        } else {
            Err(Error::BufferTooShort {
                found: self.handles.len(),
                expected_at_least: len,
            })
        }
    }

    #[inline]
    pub fn next_buffer(&mut self, len: usize) -> Result<&[u8], Error> {
        self.check_buffer(len)?;
        Ok(unsafe { self.next_buffer_unchecked(len) })
    }

    /// # Safety
    ///
    /// `len` must be less than or equal to the length of the buffer in the
    /// deserializer. Be sure to `check_buffer` first.
    pub unsafe fn next_buffer_unchecked(&mut self, len: usize) -> &[u8] {
        let (ret, next) = self.buffer.split_at_unchecked(len);
        self.buffer = next;
        ret
    }

    #[inline]
    pub fn next_handle(&mut self) -> Result<Handle, Error> {
        self.check_handles(1)?;
        Ok(unsafe { self.next_handle_unchecked() })
    }

    /// Returns the next handle unchecked of this [`Deserializer`].
    ///
    /// # Safety
    ///
    /// The deserializer must have at least one handle to be returned. Be sure
    /// to `check_handles` first.
    pub unsafe fn next_handle_unchecked(&mut self) -> Handle {
        let ret = *self.handles.get_unchecked(0);
        self.handles = self.handles.get_unchecked(1..);
        ret
    }
}

pub trait SerdePacket: Sized {
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error>;

    fn deserialize(de: &mut Deserializer) -> Result<Self, Error>;

    /// # Safety
    ///
    /// The deserializer must have enough buffer and handles to be deserialized.
    #[inline]
    unsafe fn deserialize_unchecked(de: &mut Deserializer) -> Result<Self, Error> {
        match Self::deserialize(de) {
            Err(Error::BufferTooShort { .. }) => unreachable!(),
            res => res,
        }
    }
}

pub trait Method: SerdePacket {
    const METHOD_ID: usize;
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
    #[inline]
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        ser.extend_one(self as u8);
        Ok(())
    }

    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        let byte = de.next_buffer(1)?;
        Ok(match byte[0] {
            0 => false,
            1 => true,
            byte => Err(Error::TypeMismatch(
                format!("expected bool (0 or 1), found {byte}").into(),
            ))?,
        })
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

impl<T: SerdePacket, E: SerdePacket> SerdePacket for Result<T, E> {
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        match self {
            Ok(t) => {
                0u8.serialize(ser)?;
                t.serialize(ser)?;
            }
            Err(e) => {
                1u8.serialize(ser)?;
                e.serialize(ser)?;
            }
        }
        Ok(())
    }

    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        let index = u8::deserialize(de)?;
        let ret = match index {
            0 => Ok(T::deserialize(de)?),
            1 => Err(E::deserialize(de)?),
            _ => {
                return Err(Error::TypeMismatch(
                    format!("expected result index (0 or 1), found {index}").into(),
                ))
            }
        };
        Ok(ret)
    }
}

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

impl SerdePacket for CString {
    #[inline]
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        self.into_bytes_with_nul().serialize(ser)
    }

    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        let bytes = Vec::<u8>::deserialize(de)?;
        CString::from_vec_with_nul(bytes).map_err(|err| Error::TypeMismatch(err.into()))
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

pub fn serialize<T: Method>(data: T, output: &mut Packet) -> Result<(), Error> {
    output.clear();
    let mut ser = Serializer(output);
    MAGIC.serialize(&mut ser)?;
    T::METHOD_ID.serialize(&mut ser)?;
    data.serialize(&mut ser)?;
    Ok(())
}

pub fn deserialize<T: Method>(input: &Packet, extra: Option<&mut [usize; 2]>) -> Result<T, Error> {
    let (data, de) = de_inner(input)?;
    if let Some(extra) = extra {
        *extra = [de.buffer.len(), de.handles.len()];
    }
    Ok(data)
}

pub fn deserialize_exact<T: Method>(input: &Packet) -> Result<T, Error> {
    let (data, de) = de_inner(input)?;
    if !de.is_empty() {
        return Err(Error::SizeMismatch {
            extra_buffer_len: de.buffer.len(),
            extra_handle_count: de.handles.len(),
        });
    }
    Ok(data)
}

fn de_inner<T: Method>(input: &Packet) -> Result<(T, Deserializer), Error> {
    let mut de = Deserializer {
        buffer: &input.buffer,
        handles: &input.handles,
    };
    let magic = usize::deserialize(&mut de)?;
    if magic != MAGIC {
        return Err(Error::InvalidMagic(magic));
    }
    let m = usize::deserialize(&mut de)?;
    if m != T::METHOD_ID {
        return Err(Error::InvalidMethod {
            expected: T::METHOD_ID,
            found: m,
        });
    }
    let data = T::deserialize(&mut de)?;
    Ok((data, de))
}

#[cfg(test)]
mod test {
    use alloc::{collections::BTreeMap, string::String};

    use super::{deserialize, serialize, Method, SerdePacket};
    use crate::packet::{Deserializer, Serializer};

    #[test]
    fn test_btree_map() {
        struct M<K, V>(BTreeMap<K, V>);
        impl<K: Ord + SerdePacket, V: SerdePacket> Method for M<K, V> {
            const METHOD_ID: usize = 12345;
        }
        impl<K: Ord + SerdePacket, V: SerdePacket> SerdePacket for M<K, V> {
            fn serialize(self, ser: &mut Serializer) -> Result<(), crate::Error> {
                self.0.serialize(ser)
            }

            fn deserialize(de: &mut Deserializer) -> Result<Self, crate::Error> {
                BTreeMap::deserialize(de).map(M)
            }
        }
        let ser: BTreeMap<_, _> = [
            (1, (String::from("12345"), true)),
            (2, (String::from("67890"), false)),
        ]
        .into_iter()
        .collect();
        let mut packet = Default::default();
        serialize(M(ser.clone()), &mut packet).expect("Failed to serialize packet");
        let M(de) = deserialize(&packet, None).expect("Failed to deserialize packet");

        assert_eq!(de, ser);
    }
}
