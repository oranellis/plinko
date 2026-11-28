//! The only thing that changes a tenant, so updates are logged and sent in one order.

// modules {{{
use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::{
    common::{crud::Crud, short_id::ShortId},
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
    /// Answers `None` if there is no such user
    Subscribe {
        user: Uuid,
        token: String,
        out: mpsc::UnboundedSender<ServerMessage>,
        reply: oneshot::Sender<Option<Subscriber>>,
    },
    Unsubscribe(Subscriber),
    SessionEnded(String),
    SessionsOfEnded(Uuid),
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
    /// Checks whether the day has changed
    Tick,
    /// From the scheduling thread, as of update `seq`. `solution` is `None` when already saved.
    Scheduled {
        seq: u64,
        schedule: TenantVolatileState,
        solution: Option<Solution>,
    },
}

// Command supporting types {{{
pub type Subscriber = u64;

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
        token: String,
        out: mpsc::UnboundedSender<ServerMessage>,
    ) -> Option<Subscriber> {
        let (reply, answer) = oneshot::channel();
        let subscribe = Command::Subscribe {
            user,
            token,
            out,
            reply,
        };
        self.commands.send(subscribe).ok()?;
        answer.await.ok()?
    }
}
// }}}

/// A client's socket, which closes once the actor drops `out`
struct Connection {
    user: Uuid,
    token: String,
    out: mpsc::UnboundedSender<ServerMessage>,
}

struct Actor {
    tenant: Tenant,
    store: Arc<Store>,
    seq: u64,
    connections: HashMap<Subscriber, Connection>,
    next_connection: Subscriber,
    /// The update it is for, which trails the tenant
    schedule: Option<(u64, TenantVolatileState)>,
    scheduler: worker::Scheduler,
}
// }}}

// start {{{
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
    // bring the day up to date at startup, not a minute after
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
                Command::Subscribe {
                    user,
                    token,
                    out,
                    reply,
                } => {
                    let _ = reply.send(self.subscribe(user, token, out));
                }
                Command::Unsubscribe(id) => {
                    self.connections.remove(&id);
                }
                Command::SessionEnded(token) => {
                    self.connections.retain(|_, c| c.token != token);
                }
                Command::SessionsOfEnded(user) => {
                    self.connections.retain(|_, c| c.user != user);
                }
                Command::Update {
                    subscriber,
                    request,
                    update,
                } => self.update(subscriber, request, update),
                Command::UserByEmail { email, reply } => {
                    let mut users = self.tenant.users().values();
                    let found = users.find(|u| u.has_email(&email));
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
        token: String,
        out: mpsc::UnboundedSender<ServerMessage>,
    ) -> Option<Subscriber> {
        self.tenant.users().get(&user)?;
        // the session may have ended since the socket was asked for
        let live = self.store.session_user(&token).ok().flatten() == Some(user);
        live.then_some(())?;
        let id = self.next_connection;
        self.next_connection += 1;
        let connection = Connection { user, token, out };
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
        if let TenantUpdateType::User(Crud::Remove(removed)) = copy.update_type {
            if let Err(error) = self.store.end_sessions_of(removed) {
                eprintln!("the sessions of a removed user did not end: {error}");
            }
            self.connections.retain(|_, c| c.user != removed);
        }
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
        // a new dependency can make others redundant; those go as updates of their own
        let extras = self.tenant.simplifications(&copy.update_type);
        let _ = self.scheduler.send(Applied { seq, update: copy });
        for extra in extras {
            if let Err(error) = self.apply(actor, extra) {
                eprintln!("a dependency was not simplified: {error}");
            }
        }
        Ok((seq, created))
    }

    /// Only the server moves the date, acting as the first admin
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
/// `None` for an update to the tenant as a whole
fn plan_of(update: &TenantUpdate) -> Option<ShortId> {
    match &update.update_type {
        TenantUpdateType::Plan(PlanTenantUpdate::Add(new)) => Some(new.id),
        TenantUpdateType::Plan(PlanTenantUpdate::Update(id, _) | PlanTenantUpdate::Remove(id))
        | TenantUpdateType::Favourite(id, _) => Some(*id),
        _ => None,
    }
}
// }}}
