//! A plan's settings: what new items start from, its targets, who is in it and how they are tagged, the days
//! work may be done on, and removing it.

// modules {{{
use std::collections::HashSet;

use leptos::prelude::*;
use plinko::{
    common::{crud::Crud, short_id::ShortId},
    multitenancy::tenant_update::{PlanTenantUpdate, TenantUpdateType},
    planner::resources::{
        affinity::NewAffinity,
        tag::NewTag,
        target::{NewTarget, TargetPatch},
        user::{NewPlanUser, NewPlanUserGroup, PlanUserDataPatch},
    },
    planner::{plan::PlanSchedulerDataPatch, plan_update::PlanUpdate},
};
use uuid::Uuid;
use wasm_bindgen::JsCast;

use crate::{
    availability::{AffinityWindow, AvailabilityWindow},
    icons::{Icon, IconKind},
    model::{self, node_name},
    nav::{Availability, Route, navigate},
    ui::{Choice, Field, Modal, Number, Picker, Select, Text, use_sync, window_listener},
};
// }}}

// PlanSettings {{{
#[component]
pub fn PlanSettings(plan: ShortId) -> impl IntoView {
    let sync = use_sync();
    let current = sync.tenant.with_untracked(|t| {
        t.as_ref().and_then(|t| t.plans().get(&plan)).map(|p| {
            (
                p.scheduler_data().default_start(),
                p.scheduler_data().priority(),
            )
        })
    });
    let Some((default_start, priority)) = current else {
        return view! { <p>"This plan is gone."</p> }.into_any();
    };
    let hours = RwSignal::new(false);
    let default_start = RwSignal::new(
        default_start
            .map(|n| n.id().to_string())
            .unwrap_or_default(),
    );
    let starts = Signal::derive(move || {
        let none = Choice::new("", "None", "");
        let nodes = sync.tenant.with(|t| {
            let p = t.as_ref()?.plans().get(&plan)?;
            let mut named: Vec<Choice> = model::nodes(p)
                .into_iter()
                .map(|n| Choice::new(n.id().to_string(), node_name(p, n), ""))
                .collect();
            named.sort_by(|a, b| a.label.cmp(&b.label).then(a.value.cmp(&b.value)));
            Some(named)
        });
        std::iter::once(none)
            .chain(nodes.unwrap_or_default())
            .collect::<Vec<_>>()
    });
    let priority = RwSignal::new(i64::from(priority));
    let save = move |_| {
        let chosen = default_start.get_untracked();
        let node = sync.tenant.with_untracked(|t| {
            let p = t.as_ref()?.plans().get(&plan)?;
            model::nodes(p)
                .into_iter()
                .find(|n| n.id().to_string() == chosen)
        });
        let patch = PlanSchedulerDataPatch {
            priority: Some(priority.get_untracked().clamp(-1000, 1000) as i32),
            default_start: Some(node),
        };
        sync.plan(plan, PlanUpdate::SchedulerData(patch));
    };
    let deleting = RwSignal::new(false);
    let remove = Callback::new(move |()| {
        deleting.set(false);
        sync.send(TenantUpdateType::Plan(PlanTenantUpdate::Remove(plan)));
        navigate(Route::Plans);
    });
    view! {
        <div class="people">
            <section>
                <h2>"Plan Settings"</h2>
                <Field label="Default Start Node (new items depend on it to begin with)">
                    <Picker
                        value=default_start
                        choices=starts
                        on_change=Callback::new(move |v| default_start.set(v))
                        placeholder="None"
                    />
                </Field>
                <Field label="Priority (higher goes first when several plans want the same people)">
                    <Number value=priority on_input=Callback::new(move |n| priority.set(n)) />
                </Field>
                <button type="button" class="primary" on:click=save>"Save Plan Settings"</button>
                <button type="button" on:click=move |_| hours.set(true)>
                    "Default Availability"
                </button>
                <Show when=move || hours.get()>
                    <AvailabilityWindow of=Availability::Plan(plan) on_close=Callback::new(move |()| hours.set(false)) />
                </Show>
            </section>
            <Targets plan=plan />
            <People plan=plan />
            <Tags plan=plan />
            <Affinities plan=plan />
            // only an admin may delete a plan
            <Show when=move || sync.me.with(|m| m.as_ref().is_some_and(|m| m.admin))>
                <section>
                    <h2>"Plan Management"</h2>
                    <button type="button" class="danger" on:click=move |_| deleting.set(true)>"Delete Plan"</button>
                </section>
            </Show>
            {move || deleting.get().then(|| {
                let name = sync.tenant.with_untracked(|t| {
                    t.as_ref().and_then(|t| t.plans().get(&plan)).map(|p| p.name().to_string())
                });
                let keep = Callback::new(move |()| deleting.set(false));
                window_listener("keydown", move |ev: web_sys::KeyboardEvent| {
                    // a button of the window that has the keyboard does its own thing
                    let taken = ev
                        .target()
                        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                        .is_some_and(|t| t.closest(".modal button").ok().flatten().is_some());
                    if ev.key() == "Enter" && !ev.default_prevented() && !taken {
                        ev.prevent_default();
                        remove.run(());
                    }
                });
                // a click off it, or escape, is to keep it, as for an item of the plan
                view! {
                    <Modal title=format!("Delete {}?", name.unwrap_or_default()) on_close=keep>
                        <div class="choices">
                            <button type="button" class="ctx" title="Keep it" aria-label="Keep it" on:click=move |_| keep.run(())>
                                <Icon kind=IconKind::Close size=40 />
                            </button>
                            <button type="button" class="ctx cancel" title="Delete it" aria-label="Delete it" on:click=move |_| remove.run(())>
                                <Icon kind=IconKind::Bin size=48 />
                            </button>
                        </div>
                    </Modal>
                }
            })}
        </div>
    }
    .into_any()
}
// }}}

// Targets {{{
#[component]
fn Targets(plan: ShortId) -> impl IntoView {
    let sync = use_sync();
    let targets = Memo::new(move |_| {
        sync.tenant
            .with(|t| {
                let p = t.as_ref()?.plans().get(&plan)?;
                let mut targets: Vec<(Uuid, String, i64)> = p
                    .targets()
                    .iter()
                    .map(|(id, t)| (*id, node_name(p, t.node()), i64::from(t.priority())))
                    .collect();
                targets.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
                Some(targets)
            })
            .unwrap_or_default()
    });
    let choices = Signal::derive(move || {
        sync.tenant
            .with(|t| {
                let p = t.as_ref()?.plans().get(&plan)?;
                let mut named: Vec<(String, String)> = model::nodes(p)
                    .into_iter()
                    .map(|n| (format!("{}", n.id()), node_name(p, n)))
                    .collect();
                named.sort_by(|a, b| a.1.cmp(&b.1));
                Some(named)
            })
            .unwrap_or_default()
    });
    let adding = RwSignal::new(String::new());
    let add = move |_| {
        let chosen = adding.get_untracked();
        let node = sync.tenant.with_untracked(|t| {
            let p = t.as_ref()?.plans().get(&plan)?;
            model::nodes(p)
                .into_iter()
                .find(|n| n.id().to_string() == chosen)
        });
        if let Some(node) = node {
            let new = NewTarget {
                id: Uuid::nil(),
                node,
                priority: 0,
            };
            sync.plan(plan, PlanUpdate::Target(Crud::Add(new)));
        }
    };
    view! {
        <section>
            <h2>"Scheduler Target"</h2>
            <p class="small">
                "A target is something the scheduler brings as early as it can, like a launch milestone. "
                "A plan with none is scheduled to finish as early as it can overall."
            </p>
            <For each=move || targets.get() key=|t| t.clone() children=move |(id, name, priority)| {
                let value = RwSignal::new(priority);
                view! {
                    <div class="row">
                        <strong>{name}</strong>
                        <span class="small">"priority"</span>
                        <Number value=value on_input=Callback::new(move |n| value.set(n)) />
                        <button type="button" on:click=move |_| {
                            let patch = TargetPatch { id, node: None, priority: Some(value.get_untracked().clamp(-1000, 1000) as i32) };
                            sync.plan(plan, PlanUpdate::Target(Crud::Update(patch)));
                        }>"Set"</button>
                        <button type="button" class="plain" aria-label="Remove target" on:click=move |_| {
                            sync.plan(plan, PlanUpdate::Target(Crud::Remove(id)));
                        }>"✕"</button>
                    </div>
                }
            } />
            <div class="row">
                <Select
                    value=adding
                    options=Signal::derive(move || {
                        let mut all = vec![(String::new(), "Pick an item".to_string())];
                        all.extend(choices.get());
                        all
                    })
                    on_change=Callback::new(move |v| adding.set(v))
                />
                <button type="button" on:click=add>"Add Target"</button>
            </div>
        </section>
    }
}
// }}}

// People {{{
#[component]
fn People(plan: ShortId) -> impl IntoView {
    let sync = use_sync();
    let members = Memo::new(move |_| {
        sync.tenant
            .with(|t| {
                let t = t.as_ref()?;
                let p = t.plans().get(&plan)?;
                let mut people: Vec<(Uuid, String, HashSet<Uuid>)> = p
                    .users()
                    .iter()
                    .map(|(id, u)| {
                        (
                            *id,
                            t.users()
                                .get(id)
                                .map_or("Someone".to_string(), |u| u.name().to_string()),
                            u.tags().clone(),
                        )
                    })
                    .collect();
                people.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
                Some(people)
            })
            .unwrap_or_default()
    });
    let tags = Memo::new(move |_| {
        sync.tenant
            .with(|t| {
                let p = t.as_ref()?.plans().get(&plan)?;
                let mut tags: Vec<(Uuid, String)> = p
                    .tags()
                    .iter()
                    .map(|(id, t)| (*id, t.name().to_string()))
                    .collect();
                tags.sort_by(|a, b| a.1.cmp(&b.1));
                Some(tags)
            })
            .unwrap_or_default()
    });
    let outside = Signal::derive(move || {
        sync.tenant
            .with(|t| {
                let t = t.as_ref()?;
                let p = t.plans().get(&plan)?;
                let mut out: Vec<(String, String)> = t
                    .users()
                    .iter()
                    .filter(|(id, _)| !p.users().contains_key(id))
                    .map(|(id, u)| (id.to_string(), u.name().to_string()))
                    .collect();
                out.sort_by(|a, b| a.1.cmp(&b.1));
                Some(out)
            })
            .unwrap_or_default()
    });
    let groups = Memo::new(move |_| {
        sync.tenant
            .with(|t| {
                let t = t.as_ref()?;
                let p = t.plans().get(&plan)?;
                let mut groups: Vec<(Uuid, String, bool)> = t
                    .user_groups()
                    .iter()
                    .map(|(id, g)| (*id, g.name().to_string(), p.user_groups().contains_key(id)))
                    .collect();
                groups.sort_by(|a, b| a.1.cmp(&b.1));
                Some(groups)
            })
            .unwrap_or_default()
    });
    let adding = RwSignal::new(String::new());
    view! {
        <section>
            <h2>"People"</h2>
            <For each=move || members.get() key=|m| format!("{m:?}") children=move |(id, name, tagged)| {
                view! {
                    <div class="row">
                        <strong>{name}</strong>
                        {move || tags.get().into_iter().map(|(tag, tag_name)| {
                            let on = tagged.contains(&tag);
                            let tagged = tagged.clone();
                            view! {
                                <label class="check">
                                    <input type="checkbox" prop:checked=on on:change=move |ev| {
                                        let mut next = tagged.clone();
                                        if event_target_checked(&ev) { next.insert(tag); } else { next.remove(&tag); }
                                        let patch = PlanUserDataPatch { id, tags: Some(next) };
                                        sync.plan(plan, PlanUpdate::User(Crud::Update(patch)));
                                    } />
                                    {tag_name}
                                </label>
                            }
                        }).collect_view()}
                        <button type="button" class="plain" aria-label="Remove from the plan" on:click=move |_| {
                            sync.plan(plan, PlanUpdate::User(Crud::Remove(id)));
                        }>"✕"</button>
                    </div>
                }
            } />
            <div class="row">
                <Select
                    value=adding
                    options=Signal::derive(move || {
                        let mut all = vec![(String::new(), "Add someone".to_string())];
                        all.extend(outside.get());
                        all
                    })
                    on_change=Callback::new(move |v| adding.set(v))
                />
                <button type="button" on:click=move |_| {
                    if let Ok(id) = Uuid::parse_str(&adding.get_untracked()) {
                        sync.plan(plan, PlanUpdate::User(Crud::Add(NewPlanUser { id })));
                        adding.set(String::new());
                    }
                }>"Add"</button>
            </div>
            <Show when=move || groups.with(|g| !g.is_empty())>
                <h3>"Groups whose members can be asked for work"</h3>
                {move || groups.get().into_iter().map(|(id, name, included)| view! {
                    <label class="check">
                        <input type="checkbox" prop:checked=included on:change=move |ev| {
                            let update = if event_target_checked(&ev) {
                                PlanUpdate::UserGroup(Crud::Add(NewPlanUserGroup { id }))
                            } else {
                                PlanUpdate::UserGroup(Crud::Remove(id))
                            };
                            sync.plan(plan, update);
                        } />
                        {name}
                    </label>
                }).collect_view()}
            </Show>
        </section>
    }
}
// }}}

// Tags {{{
#[component]
fn Tags(plan: ShortId) -> impl IntoView {
    let sync = use_sync();
    let tags = Memo::new(move |_| {
        sync.tenant
            .with(|t| {
                let p = t.as_ref()?.plans().get(&plan)?;
                let mut tags: Vec<(Uuid, String)> = p
                    .tags()
                    .iter()
                    .map(|(id, t)| (*id, t.name().to_string()))
                    .collect();
                tags.sort_by(|a, b| a.1.cmp(&b.1));
                Some(tags)
            })
            .unwrap_or_default()
    });
    let name = RwSignal::new(String::new());
    view! {
        <section>
            <h2>"Tags"</h2>
            <p class="small">"Tag people by skill, then ask for \"anyone tagged\" on a task and the scheduler picks."</p>
            <ul class="items">
                <For each=move || tags.get() key=|t| t.clone() children=move |(id, tag)| view! {
                    <li class="row">
                        <strong>{tag}</strong>
                        <button type="button" class="plain" aria-label="Remove tag" on:click=move |_| {
                            sync.plan(plan, PlanUpdate::Tag(Crud::Remove(id)));
                        }>"✕"</button>
                    </li>
                } />
            </ul>
            <form class="row" on:submit=move |ev| {
                ev.prevent_default();
                let text = name.get_untracked();
                if !text.trim().is_empty() {
                    sync.plan(plan, PlanUpdate::Tag(Crud::Add(NewTag { id: Uuid::nil(), name: text.trim().to_string() })));
                    name.set(String::new());
                }
            }>
                <Text value=name on_input=Callback::new(move |v| name.set(v)) placeholder="Designer" />
                <button type="submit">"Add Tag"</button>
            </form>
        </section>
    }
}
// }}}

// Affinities {{{
#[component]
fn Affinities(plan: ShortId) -> impl IntoView {
    let sync = use_sync();
    let all = Memo::new(move |_| {
        sync.tenant
            .with(|t| {
                let p = t.as_ref()?.plans().get(&plan)?;
                let mut all: Vec<(Uuid, String)> = p
                    .affinities()
                    .iter()
                    .map(|(id, a)| (*id, a.name().to_string()))
                    .collect();
                all.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
                Some(all)
            })
            .unwrap_or_default()
    });
    let name = RwSignal::new(String::new());
    let editing = RwSignal::new(None::<Uuid>);
    view! {
        <section>
            <h2>"Days work may be done on"</h2>
            <p class="small">"Link a task to one of these and it is only worked on the days the calendar allows."</p>
            <For each=move || all.get() key=|a| a.clone() children=move |(id, label)| {
                view! {
                    <div class="row">
                        <strong>{label}</strong>
                        <button type="button" on:click=move |_| editing.set(Some(id))>"Edit calendar"</button>
                        <button type="button" class="plain" aria-label="Remove" on:click=move |_| {
                            sync.plan(plan, PlanUpdate::Affinity(Crud::Remove(id)));
                        }>"✕"</button>
                    </div>
                }
            } />
            <form class="row" on:submit=move |ev| {
                ev.prevent_default();
                let text = name.get_untracked();
                if !text.trim().is_empty() {
                    sync.plan(plan, PlanUpdate::Affinity(Crud::Add(NewAffinity { id: Uuid::nil(), name: text.trim().to_string() })));
                    name.set(String::new());
                }
            }>
                <Text value=name on_input=Callback::new(move |v| name.set(v)) placeholder="Client meeting days" />
                <button type="submit">"Add"</button>
            </form>
        </section>
               {move || editing.get().map(|id| view! {
            <AffinityWindow plan=plan affinity=id on_close=Callback::new(move |()| editing.set(None)) />
        })}
    }
}
// }}}
