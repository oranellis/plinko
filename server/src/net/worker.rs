//! Scheduling runs on its own thread with its own copy of the tenant, kept up to date with the
//! same updates. Updates that arrive while it works are applied together before the next run, so
//! a busy tenant is scheduled as often as it can be and no more. A run only places what the
//! updates changed and what depends on it, so a small edit is quick. That drifts from what
//! scheduling everything gives, so once updates stop for a moment everything is scheduled again
//! on a copy, which replaces the schedule if nothing has changed meanwhile.

use std::{
    sync::mpsc::{Receiver, RecvTimeoutError, Sender},
    time::Duration,
};

use tokio::sync::mpsc::UnboundedSender;

use crate::{
    multitenancy::{tenant::Tenant, tenant_update::TenantUpdate},
    net::actor::Command,
    scheduler::{
        self, Refreshed, Tracker,
        state::{Baseline, TenantVolatileState},
    },
    types::short_id::ShortId,
};

use std::collections::HashMap;

/// How long updates have to stop for before everything is scheduled again
const QUIET: Duration = Duration::from_millis(1500);

/// An update the actor accepted, as update `seq`
pub struct Applied {
    pub seq: u64,
    pub update: TenantUpdate,
}

enum Message {
    Applied(Applied),
    /// Everything scheduled again on a copy of the tenant as of update `seq`
    Refined {
        seq: u64,
        schedule: TenantVolatileState,
        baselines: HashMap<ShortId, Baseline>,
    },
}

/// Where to send updates for the scheduler
pub struct Scheduler(Sender<Message>);

impl Scheduler {
    pub fn send(&self, applied: Applied) -> Result<(), ()> {
        self.0.send(Message::Applied(applied)).map_err(|_| ())
    }
}

pub fn start(mut tenant: Tenant, mut seq: u64, results: UnboundedSender<Command>) -> Scheduler {
    let (sender, receiver): (Sender<Message>, Receiver<Message>) = std::sync::mpsc::channel();
    let to_self = sender.clone();
    std::thread::spawn(move || {
        let mut tracker = Tracker::default();
        scheduler::schedule(&mut tenant);
        let send = |tenant: &Tenant, seq: u64| {
            let schedule = tenant.schedule().cloned().unwrap_or_default();
            results.send(Command::Scheduled { seq, schedule }).is_ok()
        };
        if !send(&tenant, seq) {
            return;
        }
        // whether the schedule has places that only a full run would give, and whether one is on
        let (mut drifted, mut refining) = (false, false);
        loop {
            // wait for something to change, or for things to be quiet enough to schedule again
            let first = if drifted && !refining {
                match receiver.recv_timeout(QUIET) {
                    Ok(message) => message,
                    Err(RecvTimeoutError::Timeout) => {
                        refining = true;
                        let mut copy = tenant.clone();
                        let back = to_self.clone();
                        let at = seq;
                        std::thread::spawn(move || {
                            copy.clear_baselines();
                            scheduler::schedule(&mut copy);
                            let schedule = copy.schedule().cloned().unwrap_or_default();
                            let baselines = copy.baselines().clone();
                            let _ = back.send(Message::Refined {
                                seq: at,
                                schedule,
                                baselines,
                            });
                        });
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            } else {
                match receiver.recv() {
                    Ok(message) => message,
                    Err(_) => return,
                }
            };
            let mut updated = false;
            let mut refined = None;
            for message in std::iter::once(first).chain(receiver.try_iter()) {
                match message {
                    Message::Applied(applied) => {
                        // the actor accepted it, so it applies here unless the copies have diverged
                        if let Err(error) = tracker.apply(&mut tenant, applied.update) {
                            eprintln!(
                                "update {} did not apply to the scheduler's copy: {error}",
                                applied.seq
                            );
                        }
                        seq = applied.seq;
                        updated = true;
                    }
                    Message::Refined {
                        seq: at,
                        schedule,
                        baselines,
                    } => {
                        refining = false;
                        refined = Some((at, schedule, baselines));
                    }
                }
            }
            if updated {
                match tracker.schedule(&mut tenant) {
                    Refreshed::Unchanged => {}
                    Refreshed::Partly => drifted = true,
                    Refreshed::Fully => drifted = false,
                }
                if !send(&tenant, seq) {
                    return;
                }
            }
            // a full run is only any use if nothing has changed since it started
            if let Some((at, schedule, baselines)) = refined.filter(|(at, ..)| *at == seq) {
                let _ = at;
                tenant.store_schedule(schedule);
                for (id, baseline) in baselines {
                    tenant.store_baseline(id, baseline);
                }
                drifted = false;
                if !send(&tenant, seq) {
                    return;
                }
            }
        }
    });
    Scheduler(sender)
}
