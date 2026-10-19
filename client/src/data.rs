//! The extra information of an item: its description, which every one has, and the data blocks it
//! can be given, at most one of each kind: a to-do list, links and a budget. Changes are saved as
//! each box is left.

// modules {{{
use leptos::prelude::*;
use plinko::{
    common::{crud::Crud, short_id::ShortId},
    planner::items::dependency::PlanNode,
    planner::resources::data_block::{
        BlockContent, Budget, DataBlock, DataBlockPatch, Link, NewDataBlock, TodoItem,
    },
    planner::{plan::Plan, plan_update::PlanUpdate},
};
use uuid::Uuid;

use crate::{
    icons::IconKind,
    model::node_name,
    ui::{IconButton, Modal, use_sync},
};
// }}}

// constants {{{
/// The kinds of block, in the order they are listed, with what they are called
const KINDS: [(&str, &str); 3] = [
    ("todo", "To-do list"),
    ("links", "Links"),
    ("budget", "Budget"),
];
// }}}

// helpers {{{
fn kind_of(content: &BlockContent) -> &'static str {
    match content {
        BlockContent::TodoList(_) => "todo",
        BlockContent::Links(_) => "links",
        BlockContent::Budget(_) => "budget",
    }
}

fn empty(kind: &str) -> BlockContent {
    match kind {
        "links" => BlockContent::Links(Vec::new()),
        "budget" => BlockContent::Budget(Budget {
            planned: 0,
            actual: 0,
            currency: "USD".to_string(),
        }),
        _ => BlockContent::TodoList(Vec::new()),
    }
}
// }}}

// data_of {{{
/// What an item says about itself: its description, and the data blocks it holds
fn data_of(plan: &Plan, node: PlanNode) -> Option<(String, Vec<DataBlock>)> {
    let (description, blocks) = match node {
        PlanNode::Task(id) => {
            let t = plan.tasks().get(&id)?;
            (t.description(), t.data().blocks())
        }
        PlanNode::Milestone(id) => {
            let m = plan.milestones().get(&id)?;
            (m.description(), m.data().blocks())
        }
        PlanNode::TaskGroup(id) => {
            let g = plan.task_groups().get(&id)?;
            (g.description(), g.data().blocks())
        }
        PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_) => return None,
    };
    let mut blocks: Vec<DataBlock> = blocks.values().cloned().collect();
    blocks.sort_by_key(|b| {
        (
            KINDS.iter().position(|k| k.0 == kind_of(b.content())),
            *b.id(),
        )
    });
    Some((description.unwrap_or_default().to_string(), blocks))
}
// }}}

// has_data {{{
/// Whether an item can hold data, which buffers and delays cannot
pub fn has_data(node: PlanNode) -> bool {
    !matches!(node, PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_))
}
// }}}

// DataWindow {{{
#[component]
pub fn DataWindow(plan: ShortId, node: PlanNode, on_close: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let state = Memo::new(move |_| {
        sync.tenant
            .with(|t| data_of(t.as_ref()?.plans().get(&plan)?, node))
    });
    let title = Signal::derive(move || {
        let name = sync.tenant.with(|t| {
            t.as_ref()
                .and_then(|t| t.plans().get(&plan))
                .map(|p| node_name(p, node))
        });
        format!("Data of {}", name.unwrap_or_default())
    });
    let adding = RwSignal::new(false);
    let missing = move || {
        let held: Vec<&str> = state
            .with(|s| {
                s.as_ref()
                    .map(|(_, b)| b.iter().map(|b| kind_of(b.content())).collect())
            })
            .unwrap_or_default();
        KINDS
            .into_iter()
            .filter(|k| !held.contains(&k.0))
            .collect::<Vec<_>>()
    };
    let add = move |kind: &'static str, label: &'static str| {
        adding.set(false);
        let new = NewDataBlock {
            id: Uuid::nil(),
            title: label.to_string(),
            content: empty(kind),
        };
        sync.plan(plan, PlanUpdate::Data(node, Crud::Add(new)));
    };
    let description = RwSignal::new(state.get_untracked().map(|s| s.0).unwrap_or_default());
    view! {
        <Modal title=title on_close=on_close>
            <div class="data">
                <div class="databar">
                    <span class="small">"Description"</span>
                    <div class="addblock">
                        <button type="button" on:click=move |_| adding.update(|a| *a = !*a) disabled=move || missing().is_empty()>
                            "+ Add block"
                        </button>
                        <Show when=move || adding.get()>
                            <ul class="blockmenu">
                                {move || missing().into_iter().map(|(kind, label)| view! {
                                    <li><button type="button" on:click=move |_| add(kind, label)>{label}</button></li>
                                }).collect_view()}
                            </ul>
                        </Show>
                    </div>
                </div>
                <textarea
                    class="description"
                    placeholder="What this is about"
                    prop:value=move || description.get()
                    on:input=move |ev| description.set(event_target_value(&ev))
                    on:change=move |_| sync.plan(plan, PlanUpdate::Describe(node, Some(description.get_untracked())))
                ></textarea>
                <For each=move || state.get().map(|s| s.1).unwrap_or_default() key=|b| format!("{b:?}") children=move |block| {
                    view! { <BlockEditor plan=plan node=node block=block /> }
                } />
            </div>
        </Modal>
    }
}
// }}}

// BlockEditor {{{
/// One block, saved as each box in it is left
#[component]
fn BlockEditor(plan: ShortId, node: PlanNode, block: DataBlock) -> impl IntoView {
    let sync = use_sync();
    let id = *block.id();
    let title = block.title().to_string();
    let send = move |content: BlockContent| {
        let patch = DataBlockPatch {
            id,
            title: None,
            content: Some(content),
        };
        sync.plan(plan, PlanUpdate::Data(node, Crud::Update(patch)));
    };
    let remove = Callback::new(move |()| {
        sync.plan(plan, PlanUpdate::Data(node, Crud::Remove(id)));
    });
    let body = match block.content().clone() {
        BlockContent::TodoList(items) => {
            view! { <Todos items=items send=Callback::new(send) /> }.into_any()
        }
        BlockContent::Links(links) => {
            view! { <Links links=links send=Callback::new(send) /> }.into_any()
        }
        BlockContent::Budget(budget) => {
            view! { <Money budget=budget send=Callback::new(send) /> }.into_any()
        }
    };
    view! {
        <section class="block">
            <header>
                <strong>{title}</strong>
                <IconButton kind=IconKind::Close label="Remove this block" on_click=remove />
            </header>
            {body}
        </section>
    }
}
// }}}

// Todos {{{
#[component]
fn Todos(items: Vec<TodoItem>, send: Callback<BlockContent>) -> impl IntoView {
    let items = RwSignal::new(items);
    let save = move || send.run(BlockContent::TodoList(items.get_untracked()));
    view! {
        <div class="list">
            <For each=move || 0..items.with(Vec::len) key=|i| *i children=move |i| view! {
                <div class="row line">
                    <input
                        type="checkbox"
                        prop:checked=move || items.with(|l| l.get(i).is_some_and(|t| t.done))
                        on:change=move |ev| {
                            let done = event_target_checked(&ev);
                            items.update(|l| if let Some(t) = l.get_mut(i) { t.done = done });
                            save();
                        }
                    />
                    <input
                        type="text"
                        prop:value=move || items.with(|l| l.get(i).map(|t| t.text.clone()).unwrap_or_default())
                        on:input=move |ev| {
                            let text = event_target_value(&ev);
                            items.update(|l| if let Some(t) = l.get_mut(i) { t.text = text });
                        }
                        on:change=move |_| save()
                    />
                    <IconButton kind=IconKind::Close label="Remove" on_click=Callback::new(move |()| {
                        items.update(|l| { if i < l.len() { l.remove(i); } });
                        save();
                    }) />
                </div>
            } />
        </div>
        <button type="button" class="add" on:click=move |_| {
            items.update(|l| l.push(TodoItem { text: String::new(), done: false }));
        }>"+ Add item"</button>
    }
}
// }}}

// Links {{{
#[component]
fn Links(links: Vec<Link>, send: Callback<BlockContent>) -> impl IntoView {
    let links = RwSignal::new(links);
    let save = move || send.run(BlockContent::Links(links.get_untracked()));
    view! {
        <div class="list">
            <For each=move || 0..links.with(Vec::len) key=|i| *i children=move |i| view! {
                <div class="row line">
                    <input
                        type="text"
                        placeholder="Name"
                        prop:value=move || links.with(|l| l.get(i).map(|t| t.name.clone()).unwrap_or_default())
                        on:input=move |ev| {
                            let name = event_target_value(&ev);
                            links.update(|l| if let Some(t) = l.get_mut(i) { t.name = name });
                        }
                        on:change=move |_| save()
                    />
                    <input
                        type="text"
                        placeholder="https://"
                        prop:value=move || links.with(|l| l.get(i).map(|t| t.url.clone()).unwrap_or_default())
                        on:input=move |ev| {
                            let url = event_target_value(&ev);
                            links.update(|l| if let Some(t) = l.get_mut(i) { t.url = url });
                        }
                        on:change=move |_| save()
                    />
                    <IconButton kind=IconKind::Close label="Remove" on_click=Callback::new(move |()| {
                        links.update(|l| { if i < l.len() { l.remove(i); } });
                        save();
                    }) />
                </div>
            } />
        </div>
        <button type="button" class="add" on:click=move |_| {
            links.update(|l| l.push(Link { name: String::new(), url: String::new() }));
        }>"+ Add link"</button>
    }
}
// }}}

// helpers {{{
/// Money is kept in minor units, shown here as the main unit with two decimals
fn shown(minor: i64) -> String {
    format!("{}.{:02}", minor / 100, (minor % 100).abs())
}

fn parsed(text: &str) -> i64 {
    let value: f64 = text.trim().parse().unwrap_or(0.0);
    (value * 100.0).round() as i64
}
// }}}

// Money {{{
#[component]
fn Money(budget: Budget, send: Callback<BlockContent>) -> impl IntoView {
    let planned = RwSignal::new(shown(budget.planned));
    let actual = RwSignal::new(shown(budget.actual));
    let currency = RwSignal::new(budget.currency);
    let save = move || {
        send.run(BlockContent::Budget(Budget {
            planned: parsed(&planned.get_untracked()),
            actual: parsed(&actual.get_untracked()),
            currency: currency.get_untracked(),
        }));
    };
    view! {
        <div class="cols">
            <label class="field"><span>"Planned"</span>
                <input type="text" inputmode="decimal" prop:value=move || planned.get()
                    on:input=move |ev| planned.set(event_target_value(&ev)) on:change=move |_| save() />
            </label>
            <label class="field"><span>"Actual"</span>
                <input type="text" inputmode="decimal" prop:value=move || actual.get()
                    on:input=move |ev| actual.set(event_target_value(&ev)) on:change=move |_| save() />
            </label>
        </div>
        <label class="field"><span>"Currency"</span>
            <input type="text" maxlength="3" prop:value=move || currency.get()
                on:input=move |ev| currency.set(event_target_value(&ev)) on:change=move |_| save() />
        </label>
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_goes_to_minor_units_and_back() {
        assert_eq!(parsed("12.5"), 1250);
        assert_eq!(parsed(" 3 "), 300);
        assert_eq!(parsed("nonsense"), 0);
        assert_eq!(shown(1250), "12.50");
        assert_eq!(shown(5), "0.05");
    }

    #[test]
    fn every_block_kind_has_a_name_and_an_empty_start() {
        for (kind, _) in KINDS {
            assert_eq!(kind_of(&empty(kind)), kind);
        }
    }
}
// }}}
