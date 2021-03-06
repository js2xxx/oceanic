use core::hash::Hasher;

pub struct FnvHasher(u64);

const FNV_PRIME: u64 = 1099511628211;
const OFFSET_BASIS: u64 = 14695981039346656037;

impl Default for FnvHasher {
    #[inline]
    fn default() -> Self {
        Self(OFFSET_BASIS)
    }
}

impl Hasher for FnvHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(FNV_PRIME);
        }
    }
}
