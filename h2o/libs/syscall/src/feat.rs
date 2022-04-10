use crate::SerdeReg;

bitflags::bitflags! {
    #[repr(transparent)]
    pub struct Feature: u64 {
        const SEND = 1 << 0;
        const SYNC = 1 << 1;
        const READ = 1 << 2;
        const WRITE = 1 << 3;
        const EXECUTE = 1 << 4;
        const WAIT = 1 << 5;
    }
}

impl SerdeReg for Feature {
    fn encode(self) -> usize {
        self.bits as usize
    }

    fn decode(val: usize) -> Self {
        Feature::from_bits_truncate(val as u64)
    }
}
