bitflags::bitflags! {
    pub struct Feature: u64 {
        const SEND = 1 << 0;
        const SYNC = 1 << 1;
        const READ = 1 << 2;
        const WRITE = 1 << 3;
        const EXECUTE = 1 << 4;
        const WAIT = 1 << 5;
    }
}
