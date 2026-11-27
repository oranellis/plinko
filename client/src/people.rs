//! Everyone in the tenant: you at the top, with your work and hours, then everyone else as
//! cards, and the groups they are in. Everyone can look; admins change other people.

// modules {{{
use std::collections::HashSet;

use leptos::{prelude::*, task::spawn_local};
use plinko::{
    common::crud::Crud,
    multitenancy::{
        tenant_update::TenantUpdateType,
        user::{NewTenantLoginUser, Role, TenantLoginUserPatch},
        user_group::{NewTenantUserGroup, TenantUserGroupPatch},
    },
    protocol::Created,
};
use uuid::Uuid;

use crate::{
    api,
    icons::IconKind,
    nav::{Availability, Route},
    person::{AHEAD, Face, HourCells, PersonView, busy, hours_from, use_load, use_open_person},
    ui::{
        Actions, Block, Choice, Field, IconButton, Modal, Picker, Select, Text, use_sync, use_today,
    },
};
// }}}

#[derive(Clone, Debug, PartialEq)]
struct Person {
    id: Uuid,
    name: String,
}

// Person supporting types {{{
#[derive(Clone, Debug, PartialEq)]
struct Group {
    id: Uuid,
    name: String,
    parent: Option<Uuid>,
    members: Vec<Uuid>,
}

/// What the form for a person changes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormMode {
    /// Someone new, with a password to start with
    New,
    /// Name, email and whether they are an admin
    Details,
    Password,
}
// }}}

// People {{{
#[component]
pub fn People() -> impl IntoView {
    let sync = use_sync();
    let me = move || sync.me.with(|m| m.as_ref().map(|m| m.id));
    let admin = move || sync.me.with(|m| m.as_ref().is_some_and(|m| m.admin));
    let people = Memo::new(move |_| {
        sync.tenant.with(|t| {
            let mut all: Vec<Person> = t
                .as_ref()
                .map(|t| {
                    t.users()
                        .iter()
                        .map(|(id, u)| Person {
                            id: *id,
                            name: u.name().to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            all.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
            all
        })
    });
    let others = Memo::new(move |_| {
        let me = me();
        people.with(|p| {
            p.iter()
                .filter(|p| Some(p.id) != me)
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let groups = Memo::new(move |_| {
        sync.tenant.with(|t| {
            let mut all: Vec<Group> = t
                .as_ref()
                .map(|t| {
                    t.user_groups()
                        .iter()
                        .map(|(id, g)| {
                            let mut members: Vec<Uuid> = g.users().iter().copied().collect();
                            members.sort();
                            Group {
                                id: *id,
                                name: g.name().to_string(),
                                parent: g.parent(),
                                members,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            all.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
            all
        })
    });
    let adding = RwSignal::new(false);
    let editing_group = RwSignal::new(None::<Option<Uuid>>);
    view! {
        <div class="people">
            <h1>"People"</h1>
            {move || me().map(|me| view! { <section class="me"><PersonView who=me /></section> })}
            <section class="everyone">
                <div class="bar">
                    <h2>"Everyone"</h2>
                    <Show when=admin>
                        <button type="button" class="primary" on:click=move |_| adding.set(true)>"New User"</button>
                    </Show>
                </div>
                <ul class="personcards">
                    <For each=move || others.get() key=|p| (p.id, p.name.clone()) children=move |p| view! {
                        <li><PersonCard id=p.id name=p.name /></li>
                    } />
                    <li><NewStarters /></li>
                </ul>
            </section>
            <section class="groups">
                <div class="bar">
                    <h2>"Groups"</h2>
                    <Show when=admin>
                        <button type="button" on:click=move |_| editing_group.set(Some(None))>"New Group"</button>
                    </Show>
                </div>
                <Show when=move || groups.with(Vec::is_empty)>
                    <p class="small">"No groups yet."</p>
                </Show>
                <ul class="items">
                    <For each=move || groups.get() key=|g| format!("{g:?}") children=move |g| {
                        let id = g.id;
                        let open = use_open_person();
                        let inside = g.parent.and_then(|p| groups.with_untracked(|all| all.iter().find(|x| x.id == p).map(|x| x.name.clone())));
                        let faces = g.members.iter().filter_map(|m| {
                            let name = people.with_untracked(|p| p.iter().find(|p| p.id == *m).map(|p| p.name.clone()))?;
                            let m = *m;
                            let title = name.clone();
                            Some(view! {
                                <button type="button" class="facebtn" title=title on:click=move |_| open.run(m)>
                                    <Face id=m name=name size=26 />
                                </button>
                            })
                        }).collect_view();
                        view! {
                            <li class="row line group">
                                <strong>{g.name.clone()}</strong>
                                {inside.map(|i| view! { <span class="small">{format!("in {i}")}</span> })}
                                <span class="faces">{faces}</span>
                                <Show when=admin>
                                    <span class="actions-right">
                                        <IconButton kind=IconKind::Pencil label="Edit" on_click=Callback::new(move |()| editing_group.set(Some(Some(id)))) />
                                        <IconButton kind=IconKind::Close label="Remove" on_click=Callback::new(move |()| {
                                            sync.send(TenantUpdateType::UserGroup(Crud::Remove(id)));
                                        }) />
                                    </span>
                                </Show>
                            </li>
                        }
                    } />
                </ul>
            </section>
        </div>
        <Show when=move || adding.get()>
            <PersonForm which=None mode=FormMode::New on_close=Callback::new(move |()| adding.set(false)) />
        </Show>
        {move || editing_group.get().map(|which| view! {
            <GroupForm which=which people=people groups=groups on_close=Callback::new(move |_| editing_group.set(None)) />
        })}
    }
}
// }}}

// PersonCard {{{
/// Someone as a card: who they are and how booked their next two weeks are, which opens their
/// window
#[component]
fn PersonCard(id: Uuid, name: String) -> impl IntoView {
    let open = use_open_person();
    let load = use_load(id, AHEAD);
    let said = Memo::new(move |_| load.with(|l| busy(l)));
    let label = name.clone();
    view! {
        <button type="button" class="personcard" on:click=move |_| open.run(id)>
            <Face id=id name=label size=36 />
            <strong>{name}</strong>
            <span class="spark" aria-hidden="true">
                {move || load.get().into_iter().map(|d| {
                    let share = if d.capacity == 0 {
                        if d.booked > 0 { 1.2 } else { 0.0 }
                    } else {
                        (f64::from(d.booked) / f64::from(d.capacity)).min(1.2)
                    };
                    let over = d.booked > u32::from(d.capacity);
                    view! { <i class:over=over style=format!("height:{}%", (share / 1.2 * 100.0).max(4.0))></i> }
                }).collect_view()}
            </span>
            <span class=move || format!("busy {}", said.get().1)>{move || said.get().0}</span>
        </button>
    }
}
// }}}

// NewStarters {{{
/// The hours new people and plans start with, as a card of their own
#[component]
fn NewStarters() -> impl IntoView {
    let sync = use_sync();
    let today = use_today();
    let admin = move || sync.me.with(|m| m.as_ref().is_some_and(|m| m.admin));
    let days = Signal::derive(move || {
        let today = today.get();
        let from = today
            - chrono::Days::new(
                crate::format::use_preferences()
                    .week_start
                    .get()
                    .offset(today) as u64,
            );
        sync.tenant.with(|t| {
            t.as_ref()
                .map(|t| {
                    hours_from(t.default_schedule(), from)
                        .into_iter()
                        .take(14)
                        .collect()
                })
                .unwrap_or_default()
        })
    });
    let inner = move || {
        view! {
            <strong>"New starters"</strong>
            <span class="small">"The hours new people and plans start with"</span>
            <HourCells days=days today=today />
        }
    };
    move || {
        if admin() {
            view! {
                <a class="personcard starters" href=Route::Availability(Availability::Default).path()>{inner()}</a>
            }.into_any()
        } else {
            view! { <div class="personcard starters">{inner()}</div> }.into_any()
        }
    }
}
// }}}

// PersonForm {{{
#[component]
pub fn PersonForm(which: Option<Uuid>, mode: FormMode, on_close: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let existing = which.and_then(|id| {
        sync.tenant.with_untracked(|t| {
            let u = t.as_ref()?.users().get(&id)?;
            Some((
                id,
                u.name().to_string(),
                u.email().to_string(),
                u.role() == Role::Admin,
            ))
        })
    });
    let name = RwSignal::new(existing.as_ref().map(|p| p.1.clone()).unwrap_or_default());
    let email = RwSignal::new(existing.as_ref().map(|p| p.2.clone()).unwrap_or_default());
    let admin = RwSignal::new(existing.as_ref().is_some_and(|p| p.3));
    let password = RwSignal::new(String::new());
    let failure = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    // a new person once they are added, while their password is still being set
    let added = RwSignal::new(None::<Uuid>);
    // the server wants the current password for your own, which Preferences asks for
    let yourself = which.is_some() && which == sync.me.with_untracked(|m| m.as_ref().map(|m| m.id));
    // only an admin makes others admins, and not themselves any less of one
    let can_promote = sync
        .me
        .with_untracked(|m| m.as_ref().is_some_and(|m| m.admin))
        && !yourself;
    let (details, secret_too) = match mode {
        FormMode::New => (true, true),
        FormMode::Details => (true, false),
        FormMode::Password => (false, true),
    };
    let title = match mode {
        FormMode::New => "New User",
        FormMode::Details => "Edit Details",
        FormMode::Password => "Set Password",
    };
    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let role = if admin.get_untracked() {
            Role::Admin
        } else {
            Role::Member
        };
        let secret = password.get_untracked();
        // as the server has it, so a new person is not added without one
        if secret_too && secret.chars().count() < 8 {
            failure.set(Some(
                "The password must be at least 8 characters".to_string(),
            ));
            return;
        }
        if busy.get_untracked() {
            return;
        }
        failure.set(None);
        match &existing {
            Some((id, old_name, old_email, old_admin)) => {
                if details {
                    let patch = TenantLoginUserPatch {
                        id: *id,
                        name: (*old_name != name.get_untracked()).then(|| name.get_untracked()),
                        email: (*old_email != email.get_untracked()).then(|| email.get_untracked()),
                        role: (can_promote && *old_admin != admin.get_untracked()).then_some(role),
                    };
                    sync.send(TenantUpdateType::User(Crud::Update(patch)));
                }
                if !secret_too {
                    on_close.run(());
                    return;
                }
                let id = *id;
                busy.set(true);
                spawn_local(async move {
                    match api::change_password(Some(id), None, &secret).await {
                        Ok(()) => _ = on_close.try_run(()),
                        Err(why) => _ = failure.try_set(Some(why)),
                    }
                    busy.try_set(false);
                });
            }
            None => {
                let new = NewTenantLoginUser {
                    id: Uuid::nil(),
                    name: name.get_untracked(),
                    email: email.get_untracked(),
                    role,
                };
                // the form stays open until the password is set too
                busy.set(true);
                let set_password = move |id: Uuid, secret: String| {
                    spawn_local(async move {
                        match api::change_password(Some(id), None, &secret).await {
                            Ok(()) => _ = on_close.try_run(()),
                            Err(why) => {
                                let why = format!("The person was added without a password: {why}");
                                if failure.try_set(Some(why.clone())).is_some() {
                                    sync.notice.set(Some(why));
                                }
                            }
                        }
                        busy.try_set(false);
                    });
                };
                // saving again after that only tries the password again
                if let Some(id) = added.get_untracked() {
                    set_password(id, secret);
                    return;
                }
                sync.send_then(TenantUpdateType::User(Crud::Add(new)), move |created| {
                    let Some(Created::Item(id)) = created else {
                        failure.try_set(Some("The person could not be added".to_string()));
                        busy.try_set(false);
                        return;
                    };
                    added.try_set(Some(id));
                    set_password(id, secret);
                });
            }
        }
    };
    view! {
        <Modal title=title on_close=on_close>
            <form class="form" on:submit=save>
                {details.then(|| view! {
                    <Field label="Name"><Text value=name on_input=Callback::new(move |v| name.set(v)) /></Field>
                    <Field label="Email"><Text value=email on_input=Callback::new(move |v| email.set(v)) kind="email" /></Field>
                })}
                {(details && (can_promote || which.is_none())).then(|| view! {
                    <label class="check">
                        <input type="checkbox" prop:checked=move || admin.get() on:change=move |ev| admin.set(event_target_checked(&ev)) />
                        "Admin (can manage people and see every plan)"
                    </label>
                })}
                {(details && yourself).then(|| view! {
                    <p class="small">
                        "Your own password is changed in "
                        <a href=Route::Preferences.path() on:click=move |_| on_close.run(())>"Preferences"</a>
                        ", given the current one"
                    </p>
                })}
                {secret_too.then(|| view! {
                    <Field label="Password, at least 8 characters">
                        <Text value=password on_input=Callback::new(move |v| password.set(v)) kind="password" />
                    </Field>
                })}
                <Show when=move || failure.get().is_some()><p class="error">{move || failure.get()}</p></Show>
                <Actions on_cancel=on_close />
            </form>
        </Modal>
    }
}
// }}}

// GroupForm {{{
#[component]
fn GroupForm(
    which: Option<Uuid>,
    people: Memo<Vec<Person>>,
    groups: Memo<Vec<Group>>,
    on_close: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    let existing =
        which.and_then(|id| groups.with_untracked(|g| g.iter().find(|g| g.id == id).cloned()));
    let name = RwSignal::new(
        existing
            .as_ref()
            .map(|g| g.name.clone())
            .unwrap_or_default(),
    );
    let parent = RwSignal::new(existing.as_ref().and_then(|g| g.parent));
    let members = RwSignal::new(
        existing
            .as_ref()
            .map(|g| g.members.iter().copied().collect::<HashSet<_>>())
            .unwrap_or_default(),
    );
    let adding = RwSignal::new(String::new());
    // the people not in the group yet
    let addable = Signal::derive(move || {
        people
            .get()
            .into_iter()
            .filter(|p| members.with(|m| !m.contains(&p.id)))
            .map(|p| Choice::new(p.id.to_string(), p.name, ""))
            .collect::<Vec<_>>()
    });
    let parents = Signal::derive(move || {
        let mut all = vec![(String::new(), "No parent group".to_string())];
        all.extend(
            groups
                .get()
                .into_iter()
                .filter(|g| Some(g.id) != which)
                .map(|g| (g.id.to_string(), g.name)),
        );
        all
    });
    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let update = match &existing {
            Some(old) => {
                let new_members = members.get_untracked();
                let old_members: HashSet<Uuid> = old.members.iter().copied().collect();
                TenantUpdateType::UserGroup(Crud::Update(TenantUserGroupPatch {
                    id: old.id,
                    name: (old.name != name.get_untracked()).then(|| name.get_untracked()),
                    parent: (old.parent != parent.get_untracked()).then(|| parent.get_untracked()),
                    users: (old_members != new_members).then_some(new_members),
                }))
            }
            None => TenantUpdateType::UserGroup(Crud::Add(NewTenantUserGroup {
                id: Uuid::nil(),
                name: name.get_untracked(),
                parent: parent.get_untracked(),
                users: members.get_untracked(),
            })),
        };
        sync.send(update);
        on_close.run(());
    };
    view! {
        <Modal title=if which.is_some() { "Edit Group" } else { "New Group" } on_close=on_close>
            <form class="form" on:submit=save>
                <Field label="Name"><Text value=name on_input=Callback::new(move |v| name.set(v)) /></Field>
                <Field label="Inside">
                    <Select
                        value=Signal::derive(move || parent.get().map(|p| p.to_string()).unwrap_or_default())
                        options=parents
                        on_change=Callback::new(move |v: String| parent.set(Uuid::parse_str(&v).ok()))
                    />
                </Field>
                <Block label="People">
                    <div class="row">
                        <Picker
                            value=adding
                            choices=addable
                            placeholder="Add someone…"
                            on_change=Callback::new(move |v: String| adding.set(v))
                        />
                        <IconButton kind=IconKind::Plus label="Add to the group" on_click=Callback::new(move |()| {
                            if let Ok(id) = Uuid::parse_str(&adding.get_untracked()) {
                                members.update(|m| { m.insert(id); });
                                adding.set(String::new());
                            }
                        }) />
                    </div>
                    <div class="list scrolling">
                        <Show when=move || members.with(|m| m.is_empty())>
                            <p class="none">"Nobody is in this group yet"</p>
                        </Show>
                        {move || people.get().into_iter().filter(|p| members.with(|m| m.contains(&p.id))).map(|p| {
                            let id = p.id;
                            view! {
                                <div class="row line">
                                    <span>{p.name}</span>
                                    <span class="actions-right">
                                        <IconButton kind=IconKind::Close label="Remove from the group" on_click=Callback::new(move |()| {
                                            members.update(|m| { m.remove(&id); });
                                        }) />
                                    </span>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                </Block>
                <Actions on_cancel=on_close />
            </form>
        </Modal>
    }
}
// }}}
