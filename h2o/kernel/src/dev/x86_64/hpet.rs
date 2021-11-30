use alloc::sync::Arc;
use core::{
    mem,
    ptr::{addr_of, NonNull},
};

// use core::ptr::addr_of_mut;
use canary::Canary;
use paging::{PAddr, PAGE_LAYOUT};
use spin::{Lazy, RwLock};

use crate::{
    cpu::time::{
        chip::{factor_from_freq, ClockChip},
        Instant,
    },
    mem::space::{self, AllocType, Flags, Phys},
};

#[repr(C, packed)]
struct HpetTimerReg {
    caps: u64,
    comparator: u64,
    route: u64,
    _rsvd: u64,
}

#[repr(C, packed)]
struct HpetReg {
    caps: u64,
    _rsvd1: u64,
    config: u64,
    _rsvd2: u64,
    status: u64,
    _rsvd3: [u8; 0xf0 - 0x28],
    counter: u64,
    _rsvd4: u64,
    timers: [HpetTimerReg; 3],
}
const HPET_REG_CFG_ENABLE: u64 = 1;

static HPET: Lazy<Option<Arc<RwLock<Hpet>>>> = Lazy::new(|| {
    acpi::HpetInfo::new(unsafe { crate::dev::acpi::tables() })
        .ok()
        .and_then(|info| unsafe { Hpet::new(info) }.ok())
        .map(|hpet| Arc::new(RwLock::new(hpet)))
});

pub struct Hpet {
    base_ptr: *mut HpetReg,
    phys: Arc<Phys>,

    block_id: u8,
    period_fs: u64,
    num_comparators: usize,
}

unsafe impl Send for Hpet {}
unsafe impl Sync for Hpet {}

impl Hpet {
    pub unsafe fn new(data: acpi::HpetInfo) -> Result<Self, &'static str> {
        struct Guard(NonNull<u8>);
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = unsafe { space::current().deallocate(self.0) };
            }
        }

        let phys = Phys::new(
            PAddr::new(data.base_address),
            PAGE_LAYOUT,
            Flags::READABLE | Flags::WRITABLE | Flags::UNCACHED,
        );
        let memory = unsafe {
            space::current()
                .allocate(
                    AllocType::Layout(phys.layout()),
                    Some(phys.clone()),
                    phys.flags(),
                )
                .expect("Failed to allocate memory")
        };
        let guard = Guard(memory.cast());
        let base_ptr = memory.cast::<HpetReg>().as_ptr();

        let num_comparators = data.num_comparators() as usize;
        if num_comparators < 2 {
            (*base_ptr).config &= !2;
            return Err("Our HPET only supports 2 or more comparators");
        }
        if ((*base_ptr).caps & (1 << 13)) == 0 {
            (*base_ptr).config &= !2;
            return Err("Our HPET only supports 64-bit counter");
        }

        let period_fs = (*base_ptr).caps >> 32;

        mem::forget(guard);
        Ok(Hpet {
            base_ptr,
            phys,
            block_id: data.hpet_number,
            period_fs,
            num_comparators,
        })
    }

    pub fn set_counter(&mut self, value: u64) -> bool {
        unsafe {
            if (*self.base_ptr).config & HPET_REG_CFG_ENABLE == 0 {
                (*self.base_ptr).counter = value;
                true
            } else {
                false
            }
        }
    }

    pub fn enable(&mut self, enabled: bool) {
        unsafe {
            if enabled {
                (*self.base_ptr).config |= HPET_REG_CFG_ENABLE;
            } else {
                (*self.base_ptr).config &= !HPET_REG_CFG_ENABLE;
            }
        }
    }

    pub fn counter(&self) -> u64 {
        let ptr = unsafe { addr_of!((*self.base_ptr).counter) };
        let a = unsafe { ptr.read_volatile() };
        let b = unsafe { ptr.read_volatile() };
        a.min(b)
    }

    pub fn phys(&self) -> &Phys {
        &self.phys
    }
}

pub fn calibrate_tsc() -> Option<u64> {
    let mut hpet = match *HPET {
        Some(ref hpet) => hpet.write(),
        None => return None,
    };

    let time_ms = 50;
    let hpet_ticks = time_ms * 1_000_000_000_000 / hpet.period_fs;

    hpet.set_counter(0);
    hpet.enable(true);
    let start = archop::msr::rdtsc();
    let mut t = start;
    while hpet.counter() < hpet_ticks {
        t = archop::msr::rdtsc();
    }
    hpet.enable(false);
    let end = t;
    Some((end - start) / time_ms)
}

pub struct HpetClock {
    canary: Canary<HpetClock>,
    hpet: Arc<RwLock<Hpet>>,
    mul: u128,
    sft: u128,
}

impl ClockChip for HpetClock {
    fn get(&self) -> Instant {
        self.canary.assert();
        let val = self.hpet.read().counter();
        unsafe { Instant::from_raw((val as u128 * self.mul) >> self.sft) }
    }
}

impl HpetClock {
    pub fn new() -> Option<HpetClock> {
        let hpet = match *HPET {
            Some(ref hpet) => hpet.clone(),
            None => return None,
        };

        let khz = 1_000_000_000_000 / hpet.read().period_fs;
        let (mul, sft) = factor_from_freq(khz);
        log::info!("HPET frequency: {} KHz", khz);

        {
            let mut hpet = hpet.write();
            hpet.set_counter(0);
            hpet.enable(true);
        }

        Some(HpetClock {
            canary: Canary::new(),
            hpet,
            mul,
            sft,
        })
    }
}
