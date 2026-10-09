//! Managing the tenant: who has a login and the groups they are in. For admins.

use std::collections::HashSet;

use leptos::{prelude::*, task::spawn_local};
use plinko::{
    multitenancy::{
        tenant_update::TenantUpdateType,
        user::{NewTenantLoginUser, Role, TenantLoginUserPatch},
        user_group::{NewTenantUserGroup, TenantUserGroupPatch},
    },
    protocol::Created,
    types::crud::Crud,
};
use uuid::Uuid;

use crate::{
    api,
    nav::{Availability, Route},
    ui::{Actions, Block, Field, Modal, Select, Text, use_sync},
};

#[derive(Clone, Debug, PartialEq)]
struct Person {
    id: Uuid,
    name: String,
    email: String,
    admin: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Group {
    id: Uuid,
    name: String,
    parent: Option<Uuid>,
    members: Vec<Uuid>,
}

#[component]
pub fn People() -> impl IntoView {
    let sync = use_sync();
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
                            email: u.email().to_string(),
                            admin: u.role() == Role::Admin,
                        })
                        .collect()
                })
                .unwrap_or_default();
            all.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
            all
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
    let editing = RwSignal::new(None::<Option<Uuid>>);
    let editing_group = RwSignal::new(None::<Option<Uuid>>);
    view! {
        <div class="people">
            <div class="bar"><h1>"People"</h1></div>
            <section>
                <div class="bar">
                    <h2>"Users"</h2>
                    <button type="button" class="primary" on:click=move |_| editing.set(Some(None))>"New User"</button>
                </div>
                <ul class="items">
                    <For each=move || people.get() key=|p| format!("{p:?}") children=move |p| {
                        let id = p.id;
                        view! {
                            <li class="row">
                                <strong>{p.name.clone()}</strong>
                                <span class="small">{p.email.clone()}{if p.admin { " · admin" } else { "" }}</span>
                                <a href=Route::Availability(Availability::User(id)).path()>"Hours"</a>
                                <button type="button" on:click=move |_| editing.set(Some(Some(id)))>"Edit"</button>
                                <Show when=move || sync.me.with(|m| m.as_ref().map(|m| m.id)) != Some(id)>
                                    <button type="button" class="plain" aria-label="Remove" on:click=move |_| {
                                        sync.send(TenantUpdateType::User(Crud::Remove(id)));
                                    }>"✕"</button>
                                </Show>
                            </li>
                        }
                    } />
                </ul>
            </section>
            <section>
                <div class="bar">
                    <h2>"Groups"</h2>
                    <button type="button" on:click=move |_| editing_group.set(Some(None))>"New Group"</button>
                </div>
                <ul class="items">
                    <For each=move || groups.get() key=|g| format!("{g:?}") children=move |g| {
                        let id = g.id;
                        view! {
                            <li class="row">
                                <strong>{g.name.clone()}</strong>
                                <span class="small">{format!("{} people", g.members.len())}</span>
                                <button type="button" on:click=move |_| editing_group.set(Some(Some(id)))>"Edit"</button>
                                <button type="button" class="plain" aria-label="Remove" on:click=move |_| {
                                    sync.send(TenantUpdateType::UserGroup(Crud::Remove(id)));
                                }>"✕"</button>
                            </li>
                        }
                    } />
                </ul>
            </section>
        </div>
        {move || editing.get().map(|which| view! {
            <PersonForm which=which people=people on_close=Callback::new(move |_| editing.set(None)) />
        })}
        {move || editing_group.get().map(|which| view! {
            <GroupForm which=which people=people groups=groups on_close=Callback::new(move |_| editing_group.set(None)) />
        })}
    }
}

#[component]
fn PersonForm(
    which: Option<Uuid>,
    people: Memo<Vec<Person>>,
    on_close: Callback<()>,
) -> impl IntoView {
    let sync = use_sync();
    let existing =
        which.and_then(|id| people.with_untracked(|p| p.iter().find(|p| p.id == id).cloned()));
    let name = RwSignal::new(
        existing
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default(),
    );
    let email = RwSignal::new(
        existing
            .as_ref()
            .map(|p| p.email.clone())
            .unwrap_or_default(),
    );
    let admin = RwSignal::new(existing.as_ref().is_some_and(|p| p.admin));
    let password = RwSignal::new(String::new());
    let failure = RwSignal::new(None::<String>);
    let title = if existing.is_some() {
        "Edit User"
    } else {
        "New User"
    };
    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let role = if admin.get_untracked() {
            Role::Admin
        } else {
            Role::Member
        };
        let secret = password.get_untracked();
        match &existing {
            Some(old) => {
                let patch = TenantLoginUserPatch {
                    id: old.id,
                    name: (old.name != name.get_untracked()).then(|| name.get_untracked()),
                    email: (old.email != email.get_untracked()).then(|| email.get_untracked()),
                    role: (old.admin != admin.get_untracked()).then_some(role),
                };
                sync.send(TenantUpdateType::User(Crud::Update(patch)));
                if secret.is_empty() {
                    on_close.run(());
                    return;
                }
                let id = old.id;
                spawn_local(async move {
                    match api::change_password(Some(id), None, &secret).await {
                        Ok(()) => on_close.run(()),
                        Err(why) => failure.set(Some(why)),
                    }
                });
            }
            None => {
                let new = NewTenantLoginUser {
                    id: Uuid::nil(),
                    name: name.get_untracked(),
                    email: email.get_untracked(),
                    role,
                };
                sync.send_then(TenantUpdateType::User(Crud::Add(new)), move |created| {
                    let Some(Created::Item(id)) = created else {
                        return;
                    };
                    spawn_local(async move {
                        if let Err(why) = api::change_password(Some(id), None, &secret).await {
                            sync.notice.set(Some(format!(
                                "The person was added without a password: {why}"
                            )));
                        }
                    });
                });
                on_close.run(());
            }
        }
    };
    view! {
        <Modal title=title on_close=on_close>
            <form class="form" on:submit=save>
                <Field label="Name"><Text value=name on_input=Callback::new(move |v| name.set(v)) /></Field>
                <Field label="Email"><Text value=email on_input=Callback::new(move |v| email.set(v)) kind="email" /></Field>
                <label class="check">
                    <input type="checkbox" prop:checked=move || admin.get() on:change=move |ev| admin.set(event_target_checked(&ev)) />
                    "Admin (can manage people and see every plan)"
                </label>
                <Field label=if which.is_some() { "New password, leave empty to keep" } else { "Password, at least 8 characters" }>
                    <Text value=password on_input=Callback::new(move |v| password.set(v)) kind="password" />
                </Field>
                <Show when=move || failure.get().is_some()><p class="error">{move || failure.get()}</p></Show>
                <Actions on_cancel=on_close />
            </form>
        </Modal>
    }
}

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
                <Block label="People"><div class="list">
                    {move || people.get().into_iter().map(|p| {
                        let id = p.id;
                        view! {
                            <label class="check">
                                <input type="checkbox" prop:checked=move || members.with(|m| m.contains(&id)) on:change=move |ev| {
                                    let on = event_target_checked(&ev);
                                    members.update(|m| { if on { m.insert(id); } else { m.remove(&id); } });
                                } />
                                {p.name}
                            </label>
                        }
                    }).collect_view()}
                </div></Block>
                <Actions on_cancel=on_close />
            </form>
        </Modal>
    }
}
