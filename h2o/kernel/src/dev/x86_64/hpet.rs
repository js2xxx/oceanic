use alloc::sync::Arc;
use core::{mem, ptr::addr_of};

// use core::ptr::addr_of_mut;
use canary::Canary;
use paging::{PAddr, PAGE_LAYOUT};
use spin::{Lazy, RwLock};

use crate::{
    cpu::time::{
        chip::{factor_from_freq, CalibrationClock, ClockChip},
        Instant,
    },
    mem::space::{self, Flags, Phys},
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
    acpi::HpetInfo::new(crate::dev::acpi::tables())
        .ok()
        .and_then(|info| unsafe { Hpet::new(info) }.ok())
        .map(|hpet| Arc::new(RwLock::new(hpet)))
});

pub static HPET_CLOCK: Lazy<Option<HpetClock>> = Lazy::new(HpetClock::new);

pub struct Hpet {
    base_ptr: *mut HpetReg,

    block_id: u8,
    period_fs: u64,
    num_comparators: usize,
}

// [`Hpet`] lives in the kernel space and should share its data.
unsafe impl Send for Hpet {}
unsafe impl Sync for Hpet {}

impl Hpet {
    unsafe fn new(data: acpi::HpetInfo) -> Result<Self, &'static str> {
        let phys = Phys::new(
            PAddr::new(data.base_address),
            PAGE_LAYOUT,
            Flags::READABLE | Flags::WRITABLE | Flags::UNCACHED,
        );
        let addr = space::KRL
            .map(
                None,
                Arc::clone(&phys),
                0,
                phys.layout().size(),
                phys.flags(),
            )
            .expect("Failed to allocate memory");
        let base_ptr = addr.cast::<HpetReg>();

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
        Ok(Hpet {
            base_ptr,
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

    pub fn block_id(&self) -> u8 {
        self.block_id
    }

    pub fn num_comparators(&self) -> usize {
        self.num_comparators
    }
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

impl CalibrationClock for HpetClock {
    unsafe fn prepare(&self, _: u64) {
        let mut hpet = self.hpet.write();
        hpet.enable(true);
        mem::forget(hpet);
    }

    unsafe fn cycle(&self, ms: u64) {
        let hpet = &mut *self.hpet.as_mut_ptr();
        let hpet_ticks = ms * 1_000_000_000_000 / hpet.period_fs;

        let start = hpet.counter();
        while hpet.counter() - start < hpet_ticks {}
    }

    unsafe fn cleanup(&self) {
        let hpet = &mut *self.hpet.as_mut_ptr();
        hpet.enable(false);
        self.hpet.force_write_unlock();
    }
}

impl HpetClock {
    pub fn new() -> Option<HpetClock> {
        let hpet = match HPET.clone() {
            Some(hpet) => hpet,
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
