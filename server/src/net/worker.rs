//! Scheduling on its own thread, with its own copy of the tenant fed the same updates.

// modules {{{
use std::sync::mpsc::{Receiver, Sender};

use tokio::sync::mpsc::UnboundedSender;

use crate::{
    multitenancy::{tenant::Tenant, tenant_update::TenantUpdate},
    net::actor::Command,
    scheduler::{self, solution::Solution},
};
// }}}

pub struct Applied {
    pub seq: u64,
    pub update: TenantUpdate,
}

// Applied supporting types {{{
pub struct Scheduler(Sender<Applied>);
// }}}

// impl Scheduler {{{
impl Scheduler {
    pub fn send(&self, applied: Applied) -> Result<(), ()> {
        self.0.send(applied).map_err(|_| ())
    }
}
// }}}

// start {{{
/// `saved` is the last solution saved and the update it was for
pub fn start(
    mut tenant: Tenant,
    mut seq: u64,
    saved: Option<(u64, Solution)>,
    results: UnboundedSender<Command>,
) -> Scheduler {
    let (sender, receiver): (Sender<Applied>, Receiver<Applied>) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut saved = saved;
        loop {
            // a solution saved for the current update is restored instead of run again
            let solution = match saved.take() {
                Some((at, solution)) if at == seq => {
                    scheduler::restore(&mut tenant, &solution);
                    None
                }
                stale => {
                    if let Some((_, solution)) = stale {
                        tenant.store_hints(solution.hints);
                    }
                    Some(scheduler::schedule(&mut tenant))
                }
            };
            let schedule = tenant.schedule().cloned().unwrap_or_default();
            let scheduled = Command::Scheduled {
                seq,
                schedule,
                solution,
            };
            if results.send(scheduled).is_err() {
                return;
            }
            let Ok(first) = receiver.recv() else {
                return;
            };
            // everything that arrived during the run is applied before the next one, so a busy
            // tenant is scheduled as often as it can be and no more
            for applied in std::iter::once(first).chain(receiver.try_iter()) {
                if let Err(error) = tenant.apply_logged(applied.update) {
                    let seq = applied.seq;
                    eprintln!("update {seq} did not apply to the scheduler's copy: {error}");
                }
                seq = applied.seq;
            }
        }
    });
    Scheduler(sender)
}
// }}}
