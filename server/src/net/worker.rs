//! Scheduling runs on its own thread with its own copy of the tenant, kept up to date with the
//! same updates. Updates that arrive while it works are applied together before the next run, so
//! a busy tenant is scheduled as often as it can be and no more. Every run schedules everything,
//! starting from what the last one found.

// modules {{{
use std::sync::mpsc::{Receiver, Sender};

use tokio::sync::mpsc::UnboundedSender;

use crate::{
    multitenancy::{tenant::Tenant, tenant_update::TenantUpdate},
    net::actor::Command,
    scheduler,
};
// }}}

/// An update the actor accepted, as update `seq`
pub struct Applied {
    pub seq: u64,
    pub update: TenantUpdate,
}

// Applied supporting types {{{
/// Where to send updates for the scheduler
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
pub fn start(mut tenant: Tenant, mut seq: u64, results: UnboundedSender<Command>) -> Scheduler {
    let (sender, receiver): (Sender<Applied>, Receiver<Applied>) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        loop {
            scheduler::schedule(&mut tenant);
            let schedule = tenant.schedule().cloned().unwrap_or_default();
            if results.send(Command::Scheduled { seq, schedule }).is_err() {
                return;
            }
            let Ok(first) = receiver.recv() else {
                return;
            };
            for applied in std::iter::once(first).chain(receiver.try_iter()) {
                // the actor accepted it, so it applies here unless the copies have diverged
                if let Err(error) = tenant.apply_update(applied.update) {
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
