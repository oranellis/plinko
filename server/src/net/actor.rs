//! The actor of a tenant. It is the only thing that changes the tenant, so updates have an order,
//! which is the order they are logged in and sent to clients in.

// modules {{{
use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::{
    common::short_id::ShortId,
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::Role,
    },
    net::worker::{self, Applied},
    protocol::{Created, Profile, ServerMessage},
    scheduler::{solution::Solution, state::TenantVolatileState},
    store::Store,
};
// }}}

pub enum Command {
    /// A client connected as `user`. The answer is `None` if there is no such user.
    Subscribe {
        user: Uuid,
        out: mpsc::UnboundedSender<ServerMessage>,
        reply: oneshot::Sender<Option<Subscriber>>,
    },
    Unsubscribe(Subscriber),
    Update {
        subscriber: Subscriber,
        request: u32,
        update: TenantUpdateType,
    },
    UserByEmail {
        email: String,
        reply: oneshot::Sender<Option<Uuid>>,
    },
    Profile {
        user: Uuid,
        reply: oneshot::Sender<Option<Profile>>,
    },
    /// Time to see whether the day has changed
    Tick,
    /// From the scheduling thread: the schedule as of update `seq`, and what was decided to
    /// get it unless that is already saved
    Scheduled {
        seq: u64,
        schedule: TenantVolatileState,
        solution: Option<Solution>,
    },
}

// Command supporting types {{{
pub type Subscriber = u64;

/// How to reach the actor
#[derive(Clone)]
pub struct Handle {
    pub commands: mpsc::UnboundedSender<Command>,
    pub store: Arc<Store>,
}

// impl Handle {{{
impl Handle {
    pub async fn user_by_email(&self, email: String) -> Option<Uuid> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::UserByEmail { email, reply })
            .ok()?;
        answer.await.ok()?
    }

    pub async fn profile(&self, user: Uuid) -> Option<Profile> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(Command::Profile { user, reply }).ok()?;
        answer.await.ok()?
    }

    pub async fn subscribe(
        &self,
        user: Uuid,
        out: mpsc::UnboundedSender<ServerMessage>,
    ) -> Option<Subscriber> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Subscribe { user, out, reply })
            .ok()?;
        answer.await.ok()?
    }
}
// }}}

struct Connection {
    user: Uuid,
    out: mpsc::UnboundedSender<ServerMessage>,
}

struct Actor {
    tenant: Tenant,
    store: Arc<Store>,
    seq: u64,
    connections: HashMap<Subscriber, Connection>,
    next_connection: Subscriber,
    /// The last schedule and the update it is for, which trails the tenant
    schedule: Option<(u64, TenantVolatileState)>,
    scheduler: worker::Scheduler,
}
// }}}

// start {{{
/// Starts the actor for a tenant that has been saved in the store
pub fn start(store: Arc<Store>) -> crate::common::error::PlinkoResult<Handle> {
    let (tenant, seq) = store.load()?;
    let (scheduler_copy, _) = store.load()?;
    let saved = store.schedule_at(seq)?;
    let (commands, receiver) = mpsc::unbounded_channel();
    let scheduler = worker::start(scheduler_copy, seq, saved, commands.clone());
    let actor = Actor {
        tenant,
        store: store.clone(),
        seq,
        connections: HashMap::new(),
        next_connection: 0,
        schedule: None,
        scheduler,
    };
    tokio::spawn(actor.run(receiver));
    let ticks = commands.clone();
    // the day is brought up to date as soon as the server starts, not a minute after
    let _ = ticks.send(Command::Tick);
    tokio::spawn(async move {
        let minute = std::time::Duration::from_secs(60);
        let mut every_minute =
            tokio::time::interval_at(tokio::time::Instant::now() + minute, minute);
        loop {
            every_minute.tick().await;
            if ticks.send(Command::Tick).is_err() {
                return;
            }
        }
    });
    Ok(Handle { commands, store })
}
// }}}

// impl Actor {{{
impl Actor {
    async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        while let Some(command) = commands.recv().await {
            match command {
                Command::Subscribe { user, out, reply } => {
                    let _ = reply.send(self.subscribe(user, out));
                }
                Command::Unsubscribe(id) => {
                    self.connections.remove(&id);
                }
                Command::Update {
                    subscriber,
                    request,
                    update,
                } => self.update(subscriber, request, update),
                Command::UserByEmail { email, reply } => {
                    let mut users = self.tenant.users().values();
                    let found = users.find(|u| u.email().eq_ignore_ascii_case(&email));
                    let _ = reply.send(found.map(|u| *u.id()));
                }
                Command::Profile { user, reply } => {
                    let found = self.tenant.users().get(&user);
                    let profile = found.map(|u| Profile {
                        id: user,
                        name: u.name().to_string(),
                        admin: u.role() == Role::Admin,
                    });
                    let _ = reply.send(profile);
                }
                Command::Tick => self.tick(),
                Command::Scheduled {
                    seq,
                    schedule,
                    solution,
                } => self.scheduled(seq, schedule, solution),
            }
        }
    }

    fn snapshot(&self, user: Uuid) -> ServerMessage {
        ServerMessage::Snapshot {
            seq: self.seq,
            user,
            tenant: Box::new(self.tenant.visible_to(&user)),
        }
    }

    fn send_schedule(&self, connection: &Connection) {
        let Some((seq, schedule)) = &self.schedule else {
            return;
        };
        let visible = self.tenant.visible_plans(&connection.user);
        let schedule = schedule.visible_to(&visible);
        let _ = connection.out.send(ServerMessage::Schedule {
            seq: *seq,
            schedule: Box::new(schedule),
        });
    }

    fn subscribe(
        &mut self,
        user: Uuid,
        out: mpsc::UnboundedSender<ServerMessage>,
    ) -> Option<Subscriber> {
        self.tenant.users().get(&user)?;
        let id = self.next_connection;
        self.next_connection += 1;
        let connection = Connection { user, out };
        let _ = connection.out.send(self.snapshot(user));
        self.send_schedule(&connection);
        self.connections.insert(id, connection);
        Some(id)
    }

    fn update(&mut self, subscriber: Subscriber, request: u32, update_type: TenantUpdateType) {
        let Some(connection) = self.connections.get(&subscriber) else {
            return;
        };
        let (user, out) = (connection.user, connection.out.clone());
        if matches!(update_type, TenantUpdateType::SetToday(_)) {
            let error = "The date is always today".to_string();
            let _ = out.send(ServerMessage::Rejected { request, error });
            return;
        }
        let answer = match self.apply(user, update_type) {
            Ok((seq, created)) => ServerMessage::Ack {
                request,
                seq,
                created,
            },
            Err(error) => ServerMessage::Rejected { request, error },
        };
        let _ = out.send(answer);
    }

    /// Makes an update as `actor`: stages it, saves it, and tells everyone who may see it
    fn apply(
        &mut self,
        actor: Uuid,
        update_type: TenantUpdateType,
    ) -> Result<(u64, Option<Created>), String> {
        let last = self.schedule.as_ref().map(|(_, s)| s);
        let staged = self.tenant.stage_from(last, Utc::now(), actor, update_type);
        let plan = plan_of(&staged);
        let before: HashMap<Subscriber, _> = self
            .connections
            .iter()
            .map(|(id, c)| (*id, self.tenant.visible_plans(&c.user)))
            .collect();
        let copy = staged.clone();
        let created = Created::by(&staged.update_type);
        let seq = self
            .store
            .commit(&mut self.tenant, self.seq, staged)
            .map_err(|e| e.to_string())?;
        self.seq = seq;
        for (id, connection) in &self.connections {
            let visible = self.tenant.visible_plans(&connection.user);
            let message = if visible != before[id] {
                self.snapshot(connection.user)
            } else if plan.is_none_or(|p| visible.contains(&p)) {
                ServerMessage::Update {
                    seq,
                    update: copy.clone(),
                }
            } else {
                continue;
            };
            let _ = connection.out.send(message);
        }
        // a new dependency can make others redundant, which go as updates of their own
        let extras = self.tenant.simplifications(&copy.update_type);
        let _ = self.scheduler.send(Applied { seq, update: copy });
        for extra in extras {
            if let Err(error) = self.apply(actor, extra) {
                eprintln!("a dependency was not simplified: {error}");
            }
        }
        Ok((seq, created))
    }

    /// Sets the tenant's clock to today when it is not, as the first admin. Only the server does
    /// this, a client cannot set the date.
    fn tick(&mut self) {
        let today = Utc::now().date_naive();
        let users = self.tenant.users().iter();
        let admin = users
            .filter(|(_, u)| u.role() == Role::Admin)
            .map(|(id, _)| *id)
            .min();
        let (true, Some(admin)) = (today != self.tenant.today(), admin) else {
            return;
        };
        if let Err(error) = self.apply(admin, TenantUpdateType::SetToday(today)) {
            eprintln!("the clock did not move on: {error}");
        }
    }

    fn scheduled(&mut self, seq: u64, schedule: TenantVolatileState, solution: Option<Solution>) {
        if let Some(Err(error)) = solution.map(|s| self.store.save_schedule(seq, &s)) {
            eprintln!("the schedule of update {seq} was not saved: {error}");
        }
        self.schedule = Some((seq, schedule));
        for connection in self.connections.values() {
            self.send_schedule(connection);
        }
    }
}
// }}}

// plan_of {{{
/// The plan an update is about, `None` for the tenant as a whole
fn plan_of(update: &TenantUpdate) -> Option<ShortId> {
    match &update.update_type {
        TenantUpdateType::Plan(PlanTenantUpdate::Add(new)) => Some(new.id),
        TenantUpdateType::Plan(PlanTenantUpdate::Update(id, _) | PlanTenantUpdate::Remove(id)) => {
            Some(*id)
        }
        _ => None,
    }
}
// }}}
