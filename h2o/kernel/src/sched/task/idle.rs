use super::*;

use spin::Lazy;

#[thread_local]
pub(super) static IDLE: Lazy<Tid> = Lazy::new(|| {
      let cpu = unsafe { crate::cpu::id() };

      let ti = TaskInfo::new(
            *ROOT,
            format!("IDLE{}", cpu),
            Type::Kernel,
            crate::cpu::current_mask(),
            prio::IDLE,
      );

      let space = unsafe { crate::mem::space::current().duplicate(Type::Kernel) };
      let entry = LAddr::new(idle as *mut u8);

      let (init, _) = Init::new(
            ti,
            space,
            entry,
            DEFAULT_STACK_SIZE,
            Some(paging::LAddr::from(
                  unsafe { archop::msr::read(archop::msr::FS_BASE) } as usize,
            )),
            &[cpu as u64],
      )
      .expect("Failed to initialize IDLE");
      let tid = init.tid;

      let mut sched = crate::sched::SCHED.lock();
      sched.push(init);

      tid
});

fn idle(cpu: usize) -> ! {
      log::debug!("IDLE #{}", cpu);
      loop {
            unsafe { archop::pause() }
      }
}
