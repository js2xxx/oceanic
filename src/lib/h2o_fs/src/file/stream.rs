use alloc::boxed::Box;
use core::ops::Deref;

use async_trait::async_trait;
use solvent::prelude::{Channel, Phys};
use solvent_async::io::Stream;
use solvent_core::{io::SeekFrom, sync::Arsc};
use solvent_rpc::io::{file::PhysOptions, Error};

use super::File;

pub struct FileStream {
    _conn: Channel,
    stream: Stream,
}

impl FileStream {
    /// # Safety
    ///
    /// `conn` must be the file client corresponding with the stream.
    #[inline]
    pub unsafe fn new(conn: Channel, stream: Stream) -> Self {
        FileStream {
            _conn: conn,
            stream,
        }
    }
}

impl Deref for FileStream {
    type Target = Stream;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

#[async_trait]
pub trait StreamIo {
    type File: File;

    fn as_file(&self) -> &Arsc<Self::File>;

    async fn lock(&mut self) -> Result<Option<Stream>, Error>;

    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error>;

    async fn read_at(&mut self, pos: usize, buf: &mut [u8]) -> Result<usize, Error>;

    async fn write(&mut self, buf: &[u8]) -> Result<usize, Error>;

    async fn write_at(&mut self, pos: usize, buf: &[u8]) -> Result<usize, Error>;

    async fn resize(&mut self, new_len: usize) -> Result<(), Error>;

    async fn seek(&mut self, pos: SeekFrom) -> Result<usize, Error>;

    async fn phys(&self, options: PhysOptions) -> Result<Phys, Error>;
}

#[cfg(feature = "runtime")]
mod runtime {
    use solvent_rpc::{
        io::file::{EventFlags, FileEventSender},
        EventSender,
    };

    use super::*;

    pub struct DirectFile<F: File> {
        inner: Arsc<F>,
        seeker: usize,
        locked: bool,
    }

    impl<F: File> DirectFile<F> {
        pub fn new(file: Arsc<F>, seeker: usize) -> Self {
            DirectFile {
                inner: file,
                seeker,
                locked: false,
            }
        }
    }

    #[async_trait]
    impl<F: File> StreamIo for DirectFile<F> {
        type File = F;
        #[inline]
        fn as_file(&self) -> &Arsc<F> {
            &self.inner
        }

        async fn lock(&mut self) -> Result<Option<Stream>, Error> {
            if self.locked {
                return Ok(None);
            }
            let res = self.inner.lock(None).await?;
            self.locked = true;
            Ok(res)
        }

        #[inline]
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
            let read_len = self.inner.read_at(self.seeker, buf).await?;
            self.seeker += read_len;
            Ok(read_len)
        }

        #[inline]
        async fn read_at(&mut self, pos: usize, buf: &mut [u8]) -> Result<usize, Error> {
            self.inner.read_at(pos, buf).await
        }

        #[inline]
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
            let written_len = self.inner.write_at(self.seeker, buf).await?;
            self.seeker += written_len;
            Ok(written_len)
        }

        #[inline]
        async fn write_at(&mut self, pos: usize, buf: &[u8]) -> Result<usize, Error> {
            self.inner.write_at(pos, buf).await
        }

        #[inline]
        async fn resize(&mut self, new_len: usize) -> Result<(), Error> {
            self.inner.resize(new_len).await
        }

        async fn seek(&mut self, pos: SeekFrom) -> Result<usize, Error> {
            fn seek_isize(start: usize, pos: isize) -> Result<usize, Error> {
                if pos > 0 {
                    Ok(start + pos as usize)
                } else {
                    let delta = (-pos) as usize;
                    start.checked_sub(delta).ok_or(Error::InvalidSeek)
                }
            }
            let new = match pos {
                SeekFrom::Start(pos) => pos,
                SeekFrom::Current(pos) => seek_isize(self.seeker, pos)?,
                SeekFrom::End(pos) => seek_isize(self.inner.len().await?, pos)?,
            };
            self.seeker = new;
            Ok(new)
        }

        #[inline]
        async fn phys(&self, options: PhysOptions) -> Result<Phys, Error> {
            self.inner.phys(options).await
        }
    }

    impl<F: File> Drop for DirectFile<F> {
        fn drop(&mut self) {
            if self.locked {
                let _ = unsafe { self.inner.unlock() };
            }
        }
    }

    pub struct StreamFile<F: File> {
        inner: Arsc<F>,
        raw: Option<Stream>,
        event: FileEventSender,
        locked: bool,
    }

    impl<F: File> StreamFile<F> {
        pub fn new(file: Arsc<F>, raw: Stream, event: FileEventSender) -> Self {
            StreamFile {
                inner: file,
                raw: Some(raw),
                event,
                locked: false,
            }
        }
    }

    impl<F: File> StreamFile<F> {
        #[inline]
        fn stream(&self) -> Result<&Stream, Error> {
            self.raw.as_ref().ok_or(Error::WouldBlock)
        }
    }

    #[async_trait]
    impl<F: File> StreamIo for StreamFile<F> {
        type File = F;
        #[inline]
        fn as_file(&self) -> &Arsc<F> {
            &self.inner
        }

        async fn lock(&mut self) -> Result<Option<Stream>, Error> {
            if self.locked {
                return Ok(None);
            }
            let res = self
                .inner
                .lock(self.raw.take().map(Stream::into_raw))
                .await?;
            self.locked = true;
            Ok(res)
        }

        #[inline]
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
            Ok(self.stream()?.read(buf).await?)
        }

        #[inline]
        async fn read_at(&mut self, pos: usize, buf: &mut [u8]) -> Result<usize, Error> {
            Ok(self.stream()?.read_at(pos, buf).await?)
        }

        #[inline]
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
            Ok(self.stream()?.write(buf).await?)
        }

        #[inline]
        async fn write_at(&mut self, pos: usize, buf: &[u8]) -> Result<usize, Error> {
            Ok(self.stream()?.write_at(pos, buf).await?)
        }

        #[inline]
        async fn resize(&mut self, new_len: usize) -> Result<(), Error> {
            Ok(self.stream()?.resize(new_len).await?)
        }

        #[inline]
        async fn seek(&mut self, pos: SeekFrom) -> Result<usize, Error> {
            Ok(self.stream()?.seek(pos).await?)
        }

        #[inline]
        async fn phys(&self, options: PhysOptions) -> Result<Phys, Error> {
            self.inner.phys(options).await
        }
    }

    impl<F: File> Drop for StreamFile<F> {
        fn drop(&mut self) {
            if self.locked {
                let _ = unsafe { self.inner.unlock() };
                let _ = self.event.send(EventFlags::UNLOCK);
            }
        }
    }
}
#[cfg(feature = "runtime")]
pub use runtime::*;
