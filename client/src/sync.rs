//! The client's copy of the tenant, kept up to date over the WebSocket. The server sends a
//! snapshot of what this user may see, then every update to it in order, which are applied with
//! the same code the server runs, and the schedule when it has one.

// modules {{{
use std::collections::HashMap;
use std::time::Duration;

use futures_channel::mpsc::{UnboundedSender, unbounded};
use futures_util::{SinkExt, StreamExt};
use gloo_net::websocket::{Message, futures::WebSocket};
use leptos::{prelude::*, task::spawn_local};
use plinko::{
    codec,
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdateType},
        user::Role,
    },
    planner::plan_update::PlanUpdate,
    protocol::{ClientMessage, Created, Profile, ServerMessage},
    scheduler::state::TenantVolatileState,
    types::short_id::ShortId,
};
// }}}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Connecting,
    Live,
    Down,
}

// Link supporting types {{{
type Then = Box<dyn FnOnce(Option<Created>)>;

#[derive(Clone, Copy)]
pub struct Sync {
    pub tenant: RwSignal<Option<Tenant>>,
    /// The schedule and the update it is for, which can trail `seq`
    pub schedule: RwSignal<Option<(u64, TenantVolatileState)>>,
    pub seq: RwSignal<u64>,
    pub me: RwSignal<Option<Profile>>,
    pub link: RwSignal<Link>,
    /// The last thing that went wrong, for a banner
    pub notice: RwSignal<Option<String>>,
    outbox: StoredValue<Option<UnboundedSender<ClientMessage>>>,
    pending: StoredValue<HashMap<u32, Then>, LocalStorage>,
    next_request: StoredValue<u32>,
    /// Set to drop the connection and start again from a snapshot
    resync: StoredValue<bool>,
}
// }}}

// impl Sync {{{
impl Sync {
    pub fn new() -> Self {
        Self {
            tenant: RwSignal::new(None),
            schedule: RwSignal::new(None),
            seq: RwSignal::new(0),
            me: RwSignal::new(None),
            link: RwSignal::new(Link::Connecting),
            notice: RwSignal::new(None),
            outbox: StoredValue::new(None),
            pending: StoredValue::new_local(HashMap::new()),
            next_request: StoredValue::new(0),
            resync: StoredValue::new(false),
        }
    }

    /// Whether the schedule is behind the latest update
    pub fn recalculating(&self) -> bool {
        let scheduled = self.schedule.with(|s| s.as_ref().map(|(seq, _)| *seq));
        scheduled.is_none_or(|scheduled| scheduled < self.seq.get())
    }

    /// Asks the server to make a change. `then` is called with what it created, or `None` if
    /// it made nothing or was refused.
    pub fn send_then(
        &self,
        update: TenantUpdateType,
        then: impl FnOnce(Option<Created>) + 'static,
    ) {
        let request = self.next_request.try_update_value(|n| {
            *n += 1;
            *n
        });
        let Some(request) = request else {
            return;
        };
        let sent = self.outbox.with_value(|outbox| {
            outbox.as_ref().is_some_and(|o| {
                o.unbounded_send(ClientMessage::Update { request, update })
                    .is_ok()
            })
        });
        if sent {
            self.pending.update_value(|p| {
                p.insert(request, Box::new(then));
            });
        } else {
            self.notice
                .set(Some("Not connected, the change was not made".to_string()));
            then(None);
        }
    }

    pub fn send(&self, update: TenantUpdateType) {
        self.send_then(update, |_| {});
    }

    /// A change to a plan
    pub fn plan(&self, plan: ShortId, update: PlanUpdate) {
        self.send(TenantUpdateType::Plan(PlanTenantUpdate::Update(
            plan,
            Box::new(update),
        )));
    }

    pub fn plan_then(
        &self,
        plan: ShortId,
        update: PlanUpdate,
        then: impl FnOnce(Option<Created>) + 'static,
    ) {
        let update = TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, Box::new(update)));
        self.send_then(update, then);
    }

    fn handle(&self, message: ServerMessage) {
        match message {
            ServerMessage::Snapshot { seq, user, tenant } => {
                let name = tenant.users().get(&user).map(|u| u.name().to_string());
                let admin = tenant
                    .users()
                    .get(&user)
                    .is_some_and(|u| u.role() == Role::Admin);
                self.me.set(name.map(|name| Profile {
                    id: user,
                    name,
                    admin,
                }));
                self.tenant.set(Some(*tenant));
                self.seq.set(seq);
                self.link.set(Link::Live);
            }
            ServerMessage::Update { seq, update } => {
                let applied = self.tenant.try_update(|tenant| match tenant {
                    Some(tenant) => tenant.apply_update(update).is_ok(),
                    None => false,
                });
                if applied != Some(true) {
                    self.resync.set_value(true);
                }
                self.seq.set(seq);
            }
            ServerMessage::Schedule { seq, schedule } => self.schedule.set(Some((seq, *schedule))),
            ServerMessage::Ack {
                request, created, ..
            } => {
                if let Some(then) = self
                    .pending
                    .try_update_value(|p| p.remove(&request))
                    .flatten()
                {
                    then(created);
                }
            }
            ServerMessage::Rejected { request, error } => {
                self.notice.set(Some(error));
                if let Some(then) = self
                    .pending
                    .try_update_value(|p| p.remove(&request))
                    .flatten()
                {
                    then(None);
                }
            }
        }
    }

    /// Keeps a connection open, starting again from a snapshot whenever it drops
    pub fn connect(&self) {
        let sync = *self;
        spawn_local(async move {
            loop {
                sync.link.set(Link::Connecting);
                if let Ok(socket) = WebSocket::open(&socket_url()) {
                    sync.run(socket).await;
                }
                sync.link.set(Link::Down);
                sync.outbox.set_value(None);
                sync.pending.update_value(|p| {
                    for (_, then) in p.drain() {
                        then(None);
                    }
                });
                // a session that has ended means logging in again, not trying again
                if crate::api::me().await.is_none() {
                    sync.me.set(None);
                    sync.tenant.set(None);
                    return;
                }
                gloo_timers::future::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    async fn run(&self, socket: WebSocket) {
        let (mut write, mut read) = socket.split();
        let (outbox, mut queue) = unbounded::<ClientMessage>();
        self.outbox.set_value(Some(outbox));
        self.resync.set_value(false);
        spawn_local(async move {
            while let Some(message) = queue.next().await {
                let Ok(bytes) = codec::encode(&message) else {
                    continue;
                };
                if write.send(Message::Bytes(bytes)).await.is_err() {
                    break;
                }
            }
        });
        while let Some(Ok(Message::Bytes(bytes))) = read.next().await {
            if let Ok(message) = codec::decode::<ServerMessage>(&bytes) {
                self.handle(message);
            }
            if self.resync.get_value() {
                break;
            }
        }
    }
}

impl Default for Sync {
    fn default() -> Self {
        Self::new()
    }
}
// }}}

// fn socket_url {{{
fn socket_url() -> String {
    let location = web_sys::window().expect("a browser").location();
    let secure = location.protocol().is_ok_and(|p| p == "https:");
    let host = location.host().unwrap_or_default();
    format!("{}://{host}/ws", if secure { "wss" } else { "ws" })
}
// }}}
