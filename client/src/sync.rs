//! The client's copy of the tenant, kept in step with the server over the WebSocket.

// modules {{{
use std::collections::HashMap;
use std::time::Duration;

use futures_channel::{
    mpsc::{UnboundedSender, unbounded},
    oneshot,
};
use futures_util::{
    SinkExt, StreamExt,
    future::{Either, select},
};
use gloo_net::websocket::{Message, futures::WebSocket};
use leptos::{prelude::*, task::spawn_local};
use plinko::{
    codec,
    common::short_id::ShortId,
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdateType},
        user::Role,
    },
    planner::plan_update::PlanUpdate,
    protocol::{ClientMessage, Created, Profile, ServerMessage},
    scheduler::state::TenantVolatileState,
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
    /// The seq is the update the schedule is for, which can trail `seq`
    pub schedule: RwSignal<Option<(u64, TenantVolatileState)>>,
    pub seq: RwSignal<u64>,
    pub me: RwSignal<Option<Profile>>,
    pub link: RwSignal<Link>,
    pub notice: RwSignal<Option<String>>,
    outbox: StoredValue<Option<UnboundedSender<ClientMessage>>>,
    pending: StoredValue<HashMap<u32, Then>, LocalStorage>,
    next_request: StoredValue<u32>,
    /// Drops the connection and starts again from a snapshot
    resync: StoredValue<bool>,
    /// Identifies the current connection loop; any other one stops
    epoch: StoredValue<u64>,
    /// Dropping it closes the open socket
    stop: StoredValue<Option<oneshot::Sender<()>>>,
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
            epoch: StoredValue::new(0),
            stop: StoredValue::new(None),
        }
    }

    pub fn end(&self) {
        self.halt();
        self.tenant.set(None);
        self.schedule.set(None);
        self.seq.set(0);
        self.me.set(None);
        self.link.set(Link::Connecting);
        self.notice.set(None);
    }

    /// Returns the epoch for a new connection loop
    fn halt(&self) -> u64 {
        let epoch = self.epoch.get_value() + 1;
        self.epoch.set_value(epoch);
        self.stop.set_value(None);
        self.outbox.set_value(None);
        self.resync.set_value(false);
        self.fail_pending();
        epoch
    }

    fn fail_pending(&self) {
        let pending = self.pending.try_update_value(std::mem::take);
        for (_, then) in pending.into_iter().flatten() {
            then(None);
        }
    }

    pub fn recalculating(&self) -> bool {
        let scheduled = self.schedule.with(|s| s.as_ref().map(|(seq, _)| *seq));
        scheduled.is_none_or(|scheduled| scheduled < self.seq.get())
    }

    /// `then` gets `None` if the server made nothing or refused
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
                // the user was removed
                let Some(name) = name else {
                    self.end();
                    return;
                };
                self.me.set(Some(Profile {
                    id: user,
                    name,
                    admin,
                }));
                // the schedule that follows is for this snapshot
                self.schedule.set(None);
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

    /// Reconnects from a snapshot whenever the link drops; stops any loop already running
    pub fn connect(&self) {
        let sync = *self;
        let epoch = sync.halt();
        let current = move || sync.epoch.get_value() == epoch;
        spawn_local(async move {
            while current() {
                sync.link.set(Link::Connecting);
                if let Ok(socket) = WebSocket::open(&socket_url()) {
                    sync.run(socket, epoch).await;
                }
                if !current() {
                    return;
                }
                sync.link.set(Link::Down);
                sync.outbox.set_value(None);
                sync.fail_pending();
                // an ended session needs a new login; an unreachable server is retried
                let ended = matches!(crate::api::me().await, Ok(None));
                if !current() {
                    return;
                }
                if ended {
                    sync.end();
                    return;
                }
                gloo_timers::future::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    async fn run(&self, socket: WebSocket, epoch: u64) {
        let (mut write, mut read) = socket.split();
        let (outbox, mut queue) = unbounded::<ClientMessage>();
        let (stop, mut stopped) = oneshot::channel();
        self.outbox.set_value(Some(outbox));
        self.stop.set_value(Some(stop));
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
        // dropping both halves closes the socket
        while let Either::Left((Some(Ok(Message::Bytes(bytes))), _)) =
            select(read.next(), &mut stopped).await
        {
            if self.epoch.get_value() != epoch {
                break;
            }
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

// socket_url {{{
fn socket_url() -> String {
    let location = web_sys::window().expect("a browser").location();
    let secure = location.protocol().is_ok_and(|p| p == "https:");
    let host = location.host().unwrap_or_default();
    format!("{}://{host}/ws", if secure { "wss" } else { "ws" })
}
// }}}
