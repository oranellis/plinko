//! A calendar of availability that rules are edited from. Days are shaded by what the rules give
//! them, a click on a day lists the rules in effect there, and a pane beside it edits them. It
//! works on the rules of a person or plan (hours) and on those of an affinity (days allowed).

use std::collections::HashMap;

use chrono::{Datelike, Days, NaiveDate, Weekday};
use leptos::prelude::*;
use plinko::{
    availability::{
        day_value::DayValue,
        rule::{
            MonthTarget, MonthlyOverride, NewRule, Ordinal, Rule, RuleKind, RulePatch,
            WeeklySchedule,
        },
        rule_set::{RuleSet, RuleSetUpdate},
    },
    types::{crud::Crud, workload::Workload},
};
use uuid::Uuid;

use crate::{
    datepicker::DateInput,
    format::{self, use_preferences},
    ui::{Field, Select, options, use_sync},
};

/// What a rule can give a day, as a level from nothing up to `MAX`
pub trait Value: DayValue + Send + Sync {
    const MAX: u8;
    fn level(&self) -> u8;
    fn from_level(level: u8) -> Self;
    /// The level said in full, as in `4 h`
    fn level_text(level: u8) -> String;
    /// The level in a cell
    fn cell_text(level: u8) -> String;
    /// What a holiday rule means
    fn off_text() -> &'static str;
}

impl Value for Workload {
    const MAX: u8 = 8;
    fn level(&self) -> u8 {
        self.get()
    }
    fn from_level(level: u8) -> Self {
        Workload::new(level.min(Self::MAX)).expect("kept within the hours of a day")
    }
    fn level_text(level: u8) -> String {
        format!("{level} h")
    }
    fn cell_text(level: u8) -> String {
        level.to_string()
    }
    fn off_text() -> &'static str {
        "Time off"
    }
}

impl Value for bool {
    const MAX: u8 = 1;
    fn level(&self) -> u8 {
        u8::from(*self)
    }
    fn from_level(level: u8) -> Self {
        level > 0
    }
    fn level_text(level: u8) -> String {
        if level > 0 { "allowed" } else { "not allowed" }.to_string()
    }
    fn cell_text(level: u8) -> String {
        if level > 0 { "✓" } else { "–" }.to_string()
    }
    fn off_text() -> &'static str {
        "Not allowed"
    }
}

const ORDINALS: [Ordinal; 10] = [
    Ordinal::First,
    Ordinal::Second,
    Ordinal::Third,
    Ordinal::Fourth,
    Ordinal::Fifth,
    Ordinal::Last,
    Ordinal::SecondLast,
    Ordinal::ThirdLast,
    Ordinal::FourthLast,
    Ordinal::FifthLast,
];

fn ordinal_name(ordinal: Ordinal) -> &'static str {
    match ordinal {
        Ordinal::First => "first",
        Ordinal::Second => "second",
        Ordinal::Third => "third",
        Ordinal::Fourth => "fourth",
        Ordinal::Fifth => "fifth",
        Ordinal::Last => "last",
        Ordinal::SecondLast => "second last",
        Ordinal::ThirdLast => "third last",
        Ordinal::FourthLast => "fourth last",
        Ordinal::FifthLast => "fifth last",
    }
}

fn weekday(index: usize) -> Weekday {
    Weekday::try_from(index as u8).unwrap_or(Weekday::Mon)
}

fn weekday_name(day: Weekday) -> &'static str {
    format::WEEKDAYS[day.num_days_from_monday() as usize]
}

/// A rule in words
pub fn describe<V: Value>(rule: &Rule<V>) -> String {
    let span = match rule.stop_day() {
        Some(stop) => format!(
            "{} to {}",
            format::day(rule.start_day()),
            format::day(stop.pred_opt().unwrap_or(stop))
        ),
        None => format!("from {}", format::day(rule.start_day())),
    };
    let what = match rule.kind() {
        RuleKind::Weekly(week) => {
            let days: Vec<String> = (0..7)
                .map(|i| {
                    let day = weekday(i);
                    format!(
                        "{} {}",
                        weekday_name(day),
                        V::cell_text(week.on(day).level())
                    )
                })
                .collect();
            format!("Weekly: {}", days.join(", "))
        }
        RuleKind::Holiday => V::off_text().to_string(),
        RuleKind::FiniteWorkdays { days, value } => {
            format!(
                "The next {days} working days: {}",
                V::level_text(value.level())
            )
        }
        RuleKind::Monthly(m) => {
            let target = match m.target {
                MonthTarget::Day => "day".to_string(),
                MonthTarget::Weekday(day) => weekday_name(day).to_string(),
                MonthTarget::Week(_) => "week".to_string(),
            };
            format!(
                "Every {} {target} of the month: {}",
                ordinal_name(m.ordinal),
                V::level_text(m.value.level())
            )
        }
    };
    format!("{what}, {span}")
}

/// The fields of a rule being written, before it is saved
#[derive(Clone, Copy)]
struct Draft {
    /// The rule being changed, `None` for a new one
    rule: Option<Uuid>,
    kind: RwSignal<String>,
    start: RwSignal<NaiveDate>,
    ends: RwSignal<bool>,
    until: RwSignal<NaiveDate>,
    week: RwSignal<[u8; 7]>,
    count: RwSignal<String>,
    level: RwSignal<u8>,
    ordinal: RwSignal<usize>,
    weekday: RwSignal<usize>,
}

impl Draft {
    fn new<V: Value>(day: NaiveDate, existing: Option<&Rule<V>>) -> Self {
        let draft = Self {
            rule: existing.map(|r| *r.id()),
            kind: RwSignal::new("weekly".to_string()),
            start: RwSignal::new(day),
            ends: RwSignal::new(false),
            until: RwSignal::new(day + Days::new(7)),
            week: RwSignal::new(V::default_week().map(|v| v.level())),
            count: RwSignal::new("5".to_string()),
            level: RwSignal::new(V::MAX / 2),
            ordinal: RwSignal::new(0),
            weekday: RwSignal::new(0),
        };
        let Some(rule) = existing else {
            return draft;
        };
        draft.start.set(rule.start_day());
        if let Some(stop) = rule.stop_day() {
            draft.ends.set(true);
            draft.until.set(stop.pred_opt().unwrap_or(stop));
        }
        match rule.kind() {
            RuleKind::Weekly(w) => draft
                .week
                .set(std::array::from_fn(|i| w.on(weekday(i)).level())),
            RuleKind::Holiday => draft.kind.set("holiday".to_string()),
            RuleKind::FiniteWorkdays { days, value } => {
                draft.kind.set("days".to_string());
                draft.count.set(days.to_string());
                draft.level.set(value.level());
            }
            RuleKind::Monthly(m) => {
                draft.kind.set("monthly".to_string());
                draft.level.set(m.value.level());
                draft
                    .ordinal
                    .set(ORDINALS.iter().position(|o| *o == m.ordinal).unwrap_or(0));
                if let MonthTarget::Weekday(day) = m.target {
                    draft.weekday.set(day.num_days_from_monday() as usize);
                }
            }
        }
        draft
    }

    fn kind<V: Value>(&self) -> RuleKind<V> {
        let level = V::from_level(self.level.get_untracked());
        match self.kind.get_untracked().as_str() {
            "holiday" => RuleKind::Holiday,
            "days" => RuleKind::FiniteWorkdays {
                days: self
                    .count
                    .get_untracked()
                    .trim()
                    .parse()
                    .unwrap_or(1)
                    .clamp(1, 3650),
                value: level,
            },
            "monthly" => RuleKind::Monthly(MonthlyOverride {
                ordinal: ORDINALS[self.ordinal.get_untracked().min(9)],
                target: MonthTarget::Weekday(weekday(self.weekday.get_untracked())),
                value: level,
                priority: 0,
            }),
            _ => RuleKind::Weekly(WeeklySchedule::new(
                self.week.get_untracked().map(V::from_level),
            )),
        }
    }

    fn update<V: Value>(&self) -> RuleSetUpdate<V> {
        let start = self.start.get_untracked();
        let until = self.until.get_untracked();
        let stop =
            (self.ends.get_untracked() && self.kind.get_untracked() != "weekly" && start <= until)
                .then(|| until + Days::new(1));
        let kind = self.kind::<V>();
        RuleSetUpdate::Rule(match self.rule {
            Some(id) => Crud::Update(RulePatch {
                id,
                start_day: Some(start),
                stop_day: Some(stop),
                kind: Some(kind),
            }),
            None => Crud::Add(NewRule {
                id: Uuid::nil(),
                start_day: start,
                stop_day: stop,
                kind,
            }),
        })
    }
}

/// The calendar and the pane beside it. `send` makes a change to the rules.
#[component]
pub fn RuleCalendar<V: Value>(
    #[prop(into)] rules: Signal<Option<RuleSet<V>>>,
    send: Callback<RuleSetUpdate<V>>,
) -> impl IntoView {
    let sync = use_sync();
    // a draft outlives the part of the page that was showing when it was made
    let owner = StoredValue::new(Owner::current());
    let make = move |day, rule: Option<&Rule<V>>| match owner.get_value() {
        Some(owner) => owner.with(|| Draft::new::<V>(day, rule)),
        None => Draft::new::<V>(day, rule),
    };
    let week_start = use_preferences().week_start;
    let today = sync
        .tenant
        .with_untracked(|t| t.as_ref().map(|t| t.today()))
        .unwrap_or_default();
    let months = RwSignal::new(12u32);
    let selected = RwSignal::new(None::<NaiveDate>);
    let editing = RwSignal::new(None::<Draft>);
    let first = today.with_day(1).unwrap_or(today);
    let last = move || {
        let n = months.get();
        let index = first.year() * 12 + first.month0() as i32 + n as i32;
        NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1)
            .unwrap_or(first)
    };
    // the level of every shown day
    let levels = Memo::new(move |_| {
        let Some(set) = rules.get() else {
            return HashMap::new();
        };
        let end = last();
        let calendar = set.calendar(first, end);
        first
            .iter_days()
            .take_while(|d| *d < end)
            .filter_map(|d| Some((d, calendar.on(d)?.level())))
            .collect::<HashMap<_, _>>()
    });
    // days a rule other than the plain weekly one touches, or any rule starts or stops on
    let marked = Memo::new(move |_| {
        let Some(set) = rules.get() else {
            return std::collections::HashSet::new();
        };
        let end = last();
        let mut marked = std::collections::HashSet::new();
        for rule in set.rules().values() {
            marked.insert(rule.start_day());
            if let Some(stop) = rule.stop_day() {
                marked.insert(stop.pred_opt().unwrap_or(stop));
            }
            if matches!(rule.kind(), RuleKind::Weekly(_)) {
                continue;
            }
            let from = rule.start_day().max(first);
            let until = rule.stop_day().map_or(end, |s| s.min(end));
            let days = from.iter_days().take_while(|d| *d < until);
            marked.extend(days.filter(|d| match rule.kind() {
                RuleKind::Holiday => true,
                _ => set.active_rules(*d).iter().any(|r| r.id() == rule.id()),
            }));
        }
        marked
    });
    let listed = Memo::new(move |_| {
        let (Some(set), Some(day)) = (rules.get(), selected.get()) else {
            return Vec::new();
        };
        set.active_rules(day)
            .into_iter()
            .map(|r| (*r.id(), describe(r)))
            .collect::<Vec<_>>()
    });
    let edit = move |id: Uuid| {
        let day = selected.get_untracked().unwrap_or(today);
        let draft = rules.with_untracked(|set| {
            set.as_ref()
                .and_then(|s| s.rules().get(&id))
                .map(|rule| make(day, Some(rule)))
        });
        editing.set(draft);
    };
    view! {
        <div class="rulecal">
            <div class="months">
                {move || {
                    let end = last();
                    let mut month = first;
                    let mut out = Vec::new();
                    while month < end {
                        let next = if month.month() == 12 {
                            NaiveDate::from_ymd_opt(month.year() + 1, 1, 1)
                        } else {
                            NaiveDate::from_ymd_opt(month.year(), month.month() + 1, 1)
                        }
                        .unwrap_or(end);
                        let blanks = week_start.get().offset(month);
                        let days: Vec<NaiveDate> = month.iter_days().take_while(|d| *d < next).collect();
                        out.push(view! {
                            <section class="month">
                                <h3>{format!("{} {}", format::month(month), month.year())}</h3>
                                <div class="days">
                                    {week_start.get().weekdays().into_iter().map(|n| view! { <span class="head">{n}</span> }).collect_view()}
                                    {(0..blanks).map(|_| view! { <span></span> }).collect_view()}
                                    {days.into_iter().map(|day| {
                                        let shade = move || {
                                            let level = levels.with(|l| l.get(&day).copied()).unwrap_or(0);
                                            format!("--shade:{}", f32::from(level) / f32::from(V::MAX))
                                        };
                                        view! {
                                            <button
                                                type="button"
                                                class="day"
                                                style=shade
                                                class:today=day == today
                                                class:picked=move || selected.get() == Some(day)
                                                title=move || {
                                                    let level = levels.with(|l| l.get(&day).copied()).unwrap_or(0);
                                                    format!("{}: {}", format::day(day), V::level_text(level))
                                                }
                                                on:click=move |_| { selected.set(Some(day)); editing.set(None); }
                                            >
                                                {day.day()}
                                                <Show when=move || marked.with(|m| m.contains(&day))>
                                                    <i class="mark"></i>
                                                </Show>
                                            </button>
                                        }
                                    }).collect_view()}
                                </div>
                            </section>
                        });
                        month = next;
                    }
                    out
                }}
                <button type="button" class="add" on:click=move |_| months.update(|m| *m += 12)>"Show more months"</button>
            </div>
            <aside class="rulepane">
                <Show
                    when=move || selected.get().is_some()
                    fallback=|| view! { <p class="small pad">"Pick a day to see and change the rules that apply."</p> }
                >
                    <div class="rule-editor">
                        {move || match editing.get() {
                            None => view! {
                                <p class="small">"Pick a rule below to change it, or add a new one."</p>
                                <button type="button" class="add" on:click=move |_| {
                                    let day = selected.get_untracked().unwrap_or(today);
                                    editing.set(Some(make(day, None)));
                                }>"+ New rule"</button>
                            }.into_any(),
                            Some(draft) => view! {
                                <RuleForm
                                    draft=draft
                                    levels=Signal::stored(
                                        (0..=V::MAX).map(|l| (l.to_string(), V::level_text(l))).collect::<Vec<_>>(),
                                    )
                                    off=V::off_text()
                                    on_save=Callback::new(move |()| {
                                        send.run(draft.update::<V>());
                                        editing.set(None);
                                    })
                                    on_cancel=Callback::new(move |()| editing.set(None))
                                />
                            }.into_any(),
                        }}
                    </div>
                    <div class="rule-list">
                        <h3>{move || selected.get().map(format::day).unwrap_or_default()}</h3>
                        <Show when=move || editing.with(|e| e.as_ref().is_some_and(|d| d.rule.is_none()))>
                            <div class="rule draft"><span>"New rule (not saved yet)"</span></div>
                        </Show>
                        <For each=move || listed.get() key=|(id, text)| format!("{id}{text}") children=move |(id, text)| view! {
                            <div class="rule" class:picked=move || editing.with(|e| e.as_ref().is_some_and(|d| d.rule == Some(id)))>
                                <span>{text}</span>
                                <button type="button" class="plain" on:click=move |_| edit(id)>"Edit"</button>
                                <button type="button" class="plain" aria-label="Delete rule" on:click=move |_| {
                                    send.run(RuleSetUpdate::Rule(Crud::Remove(id)));
                                    editing.update(|e| if e.as_ref().is_some_and(|d| d.rule == Some(id)) { *e = None });
                                }>"✕"</button>
                            </div>
                        } />
                    </div>
                </Show>
            </aside>
        </div>
    }
}

#[component]
fn RuleForm(
    draft: Draft,
    /// The values a day can be given, by level
    levels: Signal<Vec<(String, String)>>,
    /// What a rule that takes the day away is called
    off: &'static str,
    on_save: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let kind = draft.kind;
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); on_save.run(()); }>
            <Field label="What">
                <Select
                    value=kind
                    options=options([
                        ("weekly", "Weekly schedule"),
                        ("holiday", off),
                        ("days", "Set the next working days"),
                        ("monthly", "A day every month"),
                    ])
                    on_change=Callback::new(move |v| kind.set(v))
                />
            </Field>
            <Field label="From"><DateInput day=draft.start /></Field>
            <Show when=move || kind.get() != "weekly">
                <label class="check">
                    <input type="checkbox" prop:checked=move || draft.ends.get() on:change=move |ev| draft.ends.set(event_target_checked(&ev)) />
                    "Until a day"
                </label>
                <Show when=move || draft.ends.get()><Field label="Last day"><DateInput day=draft.until /></Field></Show>
            </Show>
            <Show when=move || kind.get() == "weekly">
                <div class="weekrow">
                    {(0..7).map(|i| view! {
                        <Field label=format::WEEKDAYS[i]>
                            <Select
                                value=Signal::derive(move || draft.week.with(|w| w[i].to_string()))
                                options=levels
                                on_change=Callback::new(move |v: String| {
                                    if let Ok(l) = v.parse::<u8>() { draft.week.update(|w| w[i] = l); }
                                })
                            />
                        </Field>
                    }).collect_view()}
                </div>
            </Show>
            <Show when=move || kind.get() == "days">
                <Field label="How many working days">
                    <input
                        type="number"
                        min="1"
                        prop:value=move || draft.count.get()
                        on:input=move |ev| draft.count.set(event_target_value(&ev))
                    />
                </Field>
            </Show>
            <Show when=move || kind.get() == "monthly">
                <div class="cols">
                    <Field label="Which">
                        <Select
                            value=Signal::derive(move || draft.ordinal.get().to_string())
                            options=Signal::stored(
                                ORDINALS.iter().enumerate().map(|(i, o)| (i.to_string(), ordinal_name(*o).to_string())).collect::<Vec<_>>(),
                            )
                            on_change=Callback::new(move |v: String| { if let Ok(i) = v.parse() { draft.ordinal.set(i); } })
                        />
                    </Field>
                    <Field label="Day">
                        <Select
                            value=Signal::derive(move || draft.weekday.get().to_string())
                            options=Signal::stored(
                                (0..7).map(|i| (i.to_string(), weekday_name(weekday(i)).to_string())).collect::<Vec<_>>(),
                            )
                            on_change=Callback::new(move |v: String| { if let Ok(i) = v.parse() { draft.weekday.set(i); } })
                        />
                    </Field>
                </div>
            </Show>
            <Show when=move || matches!(kind.get().as_str(), "days" | "monthly")>
                <Field label="Value">
                    <Select
                        value=Signal::derive(move || draft.level.get().to_string())
                        options=levels
                        on_change=Callback::new(move |v: String| { if let Ok(l) = v.parse() { draft.level.set(l); } })
                    />
                </Field>
            </Show>
            <div class="actions">
                <span class="grow"></span>
                <button type="button" on:click=move |_| on_cancel.run(())>"Cancel"</button>
                <button type="submit" class="primary">"Save"</button>
            </div>
        </form>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn rule<V: Value>(kind: RuleKind<V>, stop: Option<NaiveDate>) -> String {
        let mut set = RuleSet::<V>::default();
        let new = NewRule {
            id: Uuid::from_u128(1),
            start_day: d(8),
            stop_day: stop,
            kind,
        };
        set.apply_update(RuleSetUpdate::Rule(Crud::Add(new)))
            .unwrap();
        describe(set.rules().values().next().unwrap())
    }

    #[test]
    fn rules_are_described_in_words() {
        assert_eq!(
            rule::<Workload>(RuleKind::Holiday, None),
            "Time off, from Mon 8 Jan"
        );
        assert_eq!(
            rule::<Workload>(RuleKind::Holiday, Some(d(13))),
            "Time off, Mon 8 Jan to Fri 12 Jan"
        );
        let hours = Workload::new(4).unwrap();
        assert_eq!(
            rule(
                RuleKind::FiniteWorkdays {
                    days: 3,
                    value: hours
                },
                None
            ),
            "The next 3 working days: 4 h, from Mon 8 Jan"
        );
        let week = WeeklySchedule::from_hours([8, 8, 8, 8, 4, 0, 0]).unwrap();
        assert_eq!(
            rule(RuleKind::Weekly(week), None),
            "Weekly: Mon 8, Tue 8, Wed 8, Thu 8, Fri 4, Sat 0, Sun 0, from Mon 8 Jan"
        );
        assert_eq!(
            rule::<bool>(RuleKind::Holiday, None),
            "Not allowed, from Mon 8 Jan"
        );
    }

    #[test]
    fn a_draft_makes_the_rule_it_was_made_from() {
        let week = WeeklySchedule::from_hours([8, 8, 8, 8, 4, 0, 0]).unwrap();
        let mut set = RuleSet::<Workload>::default();
        let new = NewRule {
            id: Uuid::from_u128(1),
            start_day: d(8),
            stop_day: None,
            kind: RuleKind::Weekly(week.clone()),
        };
        set.apply_update(RuleSetUpdate::Rule(Crud::Add(new)))
            .unwrap();
        let rule = set.rules().values().next().unwrap();
        let draft = Draft::new(d(1), Some(rule));
        assert_eq!(draft.kind::<Workload>(), RuleKind::Weekly(week));
        assert_eq!(draft.start.get_untracked(), d(8));
    }
}
