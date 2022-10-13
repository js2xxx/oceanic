use alloc::vec::Vec;
use core::fmt;

use solvent::prelude::{IoSlice, IoSliceMut, PAGE_MASK};
use solvent_core::io::{RawStream, SeekFrom};

use crate::{disp::DispSender, mem::Phys, sync::Mutex};

pub struct Stream {
    inner: Mutex<Inner>,
}

impl fmt::Debug for Stream {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Stream").finish_non_exhaustive()
    }
}

impl From<Stream> for RawStream {
    fn from(stream: Stream) -> Self {
        let inner = stream.inner.into_inner();
        RawStream {
            phys: Phys::into_inner(inner.phys),
            len: inner.len,
            seeker: inner.seeker,
        }
    }
}

impl Stream {
    /// # Safety
    ///
    /// The stream must holds the unique reference to the `Phys`, or the others
    /// must not be operating when the stream is alive, if its memory safety is
    /// not guaranteed by the kernel.
    ///
    /// See [`Phys::write`] for more information.
    #[cfg(feature = "runtime")]
    #[inline]
    pub unsafe fn new(raw: RawStream) -> Self {
        Self::with_disp(raw, crate::dispatch())
    }

    /// # Safety
    ///
    /// The stream must holds the unique reference to the `Phys`, or the others
    /// must not be operating when the stream is alive, if its memory safety is
    /// not guaranteed by the kernel.
    ///
    /// See [`Phys::write`] for more information.
    pub unsafe fn with_disp(raw: RawStream, disp: DispSender) -> Self {
        let phys = Phys::with_disp(raw.phys, disp);
        Stream {
            inner: Mutex::new(Inner {
                phys,
                len: raw.len,
                seeker: raw.seeker,
            }),
        }
    }

    pub async fn seek(&self, pos: SeekFrom) -> Result<usize, Error> {
        self.inner.lock().await.seek(pos).await
    }

    pub async fn read(&self, buf: &mut [u8]) -> Result<usize, Error> {
        self.inner.lock().await.read(buf).await
    }

    pub async fn read_at(&self, pos: usize, buf: &mut [u8]) -> Result<usize, Error> {
        self.inner.lock().await.read_at(pos, buf).await
    }

    pub async fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize, Error> {
        self.inner.lock().await.read_vectored(bufs).await
    }

    pub async fn read_at_vectored(
        &self,
        pos: usize,
        bufs: &mut [IoSliceMut<'_>],
    ) -> Result<usize, Error> {
        self.inner.lock().await.read_at_vectored(pos, bufs).await
    }

    pub async fn write(&self, buf: &[u8]) -> Result<usize, Error> {
        self.inner.lock().await.write(buf).await
    }

    pub async fn write_at(&self, pos: usize, buf: &[u8]) -> Result<usize, Error> {
        self.inner.lock().await.write_at(pos, buf).await
    }

    pub async fn write_vectored(&self, bufs: &mut [IoSlice<'_>]) -> Result<usize, Error> {
        self.inner.lock().await.write_vectored(bufs).await
    }

    pub async fn write_at_vectored(
        &self,
        pos: usize,
        bufs: &mut [IoSlice<'_>],
    ) -> Result<usize, Error> {
        self.inner.lock().await.write_at_vectored(pos, bufs).await
    }

    pub async fn resize(&self, new_len: usize) -> Result<(), Error> {
        let mut inner = self.inner.lock().await;
        inner.len = new_len;
        let res = inner.phys.resize(new_len, true).await;
        res.map_err(Error::Other)
    }
}

#[derive(Debug)]
pub enum Error {
    Other(solvent::error::Error),
    InvalidSeek(SeekFrom),
}

struct Inner {
    phys: Phys,
    len: usize,
    seeker: usize,
}

impl Inner {
    #[inline]
    async fn seek(&mut self, pos: SeekFrom) -> Result<usize, Error> {
        match pos {
            SeekFrom::Start(start) => self.seeker = start,
            SeekFrom::Current(delta) => {
                if delta >= 0 {
                    self.seeker += delta as usize;
                } else {
                    let delta = (-delta) as usize;
                    if self.seeker < delta {
                        return Err(Error::InvalidSeek(pos));
                    }
                    self.seeker -= delta;
                }
            }
            SeekFrom::End(delta) => {
                let len = self.len;
                if delta >= 0 {
                    self.seeker = len + delta as usize;
                } else {
                    let delta = (-delta) as usize;
                    if len < delta {
                        return Err(Error::InvalidSeek(pos));
                    }
                    self.seeker = len - delta
                }
            }
        }
        Ok(self.seeker)
    }

    async fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize, Error> {
        let mut cache = Vec::new();
        let mut read_len = 0;
        for buf in bufs.iter_mut().filter(|buf| !buf.is_empty()) {
            cache.clear();
            cache.reserve(buf.len().min(self.len.saturating_sub(self.seeker)));
            self.phys
                .read(self.seeker, &mut cache)
                .await
                .map_err(Error::Other)?;
            let len = cache.len().min(buf.len());
            buf[..len].copy_from_slice(&cache[..len]);
            read_len += len;
            self.seeker += len;
            if len < buf.len() {
                break;
            }
        }
        Ok(read_len)
    }

    async fn read_at_vectored(
        &self,
        mut pos: usize,
        bufs: &mut [IoSliceMut<'_>],
    ) -> Result<usize, Error> {
        let mut cache = Vec::new();
        let mut read_len = 0;
        for buf in bufs.iter_mut().filter(|buf| !buf.is_empty()) {
            cache.clear();
            cache.reserve(buf.len().min(self.len.saturating_sub(self.seeker)));
            self.phys
                .read(pos, &mut cache)
                .await
                .map_err(Error::Other)?;
            let len = cache.len().min(buf.len());
            buf[..len].copy_from_slice(&cache[..len]);
            read_len += len;
            pos += len;
            if len < buf.len() {
                break;
            }
        }
        Ok(read_len)
    }

    #[inline]
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.read_vectored(&mut [IoSliceMut::new(buf)]).await
    }

    #[inline]
    async fn read_at(&self, pos: usize, buf: &mut [u8]) -> Result<usize, Error> {
        self.read_at_vectored(pos, &mut [IoSliceMut::new(buf)])
            .await
    }

    async fn write_inner(
        phys: &Phys,
        len: usize,
        seeker: &mut usize,
        bufs: &mut [IoSlice<'_>],
    ) -> Result<Result<usize, usize>, Error> {
        let mut cache = Vec::new();
        let mut written_len = 0;
        for buf in bufs.iter().filter(|buf| !buf.is_empty()) {
            cache.clear();
            let len = buf.len().min(len.saturating_sub(*seeker));
            cache.extend_from_slice(&buf[..len]);
            // SAFETY: This struct holds the unique reference to the underlying `Phys`.
            let len = unsafe { phys.write(*seeker, &mut cache) }
                .await
                .map_err(Error::Other)?;
            *seeker += len;
            written_len += len;
            if len < buf.len() {
                return Ok(Err(written_len));
            }
        }
        Ok(Ok(written_len))
    }

    async fn write_vectored(&mut self, mut bufs: &mut [IoSlice<'_>]) -> Result<usize, Error> {
        IoSlice::advance_slices(&mut bufs, 0);
        match Self::write_inner(&self.phys, self.len, &mut self.seeker, bufs).await? {
            Ok(written_len) => Ok(written_len),
            Err(len1) => {
                IoSlice::advance_slices(&mut bufs, len1);
                if bufs.is_empty() {
                    Ok(len1)
                } else {
                    let additional: usize = bufs.iter().map(|buf| buf.len()).sum();
                    self.len += additional;
                    self.phys
                        .resize((self.len + PAGE_MASK) & !PAGE_MASK, true)
                        .await
                        .map_err(Error::Other)?;
                    let res = Self::write_inner(&self.phys, self.len, &mut self.seeker, bufs).await;
                    res.map(|res| {
                        len1 + match res {
                            Ok(len2) => len2,
                            Err(len2) => len2,
                        }
                    })
                }
            }
        }
    }

    async fn write_at_vectored(
        &mut self,
        mut pos: usize,
        mut bufs: &mut [IoSlice<'_>],
    ) -> Result<usize, Error> {
        IoSlice::advance_slices(&mut bufs, 0);
        match Self::write_inner(&self.phys, self.len, &mut pos, bufs).await? {
            Ok(written_len) => Ok(written_len),
            Err(len1) => {
                IoSlice::advance_slices(&mut bufs, len1);
                if bufs.is_empty() {
                    Ok(len1)
                } else {
                    let additional: usize = bufs.iter().map(|buf| buf.len()).sum();
                    self.len += additional;
                    self.phys
                        .resize((self.len + PAGE_MASK) & !PAGE_MASK, true)
                        .await
                        .map_err(Error::Other)?;
                    let res = Self::write_inner(&self.phys, self.len, &mut pos, bufs).await;
                    res.map(|res| {
                        len1 + match res {
                            Ok(len2) => len2,
                            Err(len2) => len2,
                        }
                    })
                }
            }
        }
    }

    #[inline]
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.write_vectored(&mut [IoSlice::new(buf)]).await
    }

    #[inline]
    async fn write_at(&mut self, pos: usize, buf: &[u8]) -> Result<usize, Error> {
        self.write_at_vectored(pos, &mut [IoSlice::new(buf)]).await
    }
}
