//! A calendar of availability that rules are edited from. Days are shaded by what the rules give
//! them, a click on a day lists the rules in effect there, and a pane beside it edits them. It
//! works on the rules of a person or plan (hours) and on those of an affinity (days allowed).

// modules {{{
use std::collections::HashMap;

use chrono::{Datelike, Days, NaiveDate, Weekday};
use leptos::{html, prelude::*};
use plinko::{
    availability::workload::Workload,
    availability::{
        day_value::DayValue,
        rule::{
            DEFAULT_RULE, DayOverride, Every, MAX_NAME, NewRule, Rule, RuleKind, RulePatch, Unit,
            WeeklySchedule,
        },
        rule_set::{RuleSet, RuleSetUpdate},
    },
    common::crud::Crud,
};
use uuid::Uuid;

use crate::{
    datepicker::DateInput,
    format::{self, use_preferences},
    ui::{Field, Hours, Select, options, use_today},
};
// }}}

/// What a rule can give a day, as a level from nothing up to `MAX`
pub trait Value: DayValue + Send + Sync {
    const MAX: u8;
    fn level(&self) -> u8;
    fn from_level(level: u8) -> Self;
    /// The level said in full, as in `4 h`
    fn level_text(level: u8) -> String;
    /// What a holiday rule means
    fn off_text() -> &'static str;
}

// impl Value {{{
impl Value for Workload {
    const MAX: u8 = 8;
    fn level(&self) -> u8 {
        self.get()
    }
    fn from_level(level: u8) -> Self {
        Workload::new(level.min(<Self as Value>::MAX)).expect("kept within the hours of a day")
    }
    fn level_text(level: u8) -> String {
        format!("{level} h")
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
    fn off_text() -> &'static str {
        "Not allowed"
    }
}
// }}}

// helpers {{{
fn weekday(index: usize) -> Weekday {
    Weekday::try_from(index as u8).unwrap_or(Weekday::Mon)
}

const UNITS: [(Unit, &str, &str); 5] = [
    (Unit::Day, "day", "Day(s)"),
    (Unit::Week, "week", "Week(s)"),
    (Unit::Month, "month", "Month(s)"),
    (Unit::MonthAligned, "aligned", "Month(s) Aligned"),
    (Unit::Year, "year", "Year(s)"),
];

fn unit_name(unit: Unit) -> &'static str {
    UNITS.iter().find(|u| u.0 == unit).map_or("day", |u| u.1)
}

/// What a rule is called: the name it was given, or its kind
fn title<V: Value>(rule: &Rule<V>, off: &str) -> String {
    if *rule.id() == DEFAULT_RULE {
        return "Default week".to_string();
    }
    if !rule.name().is_empty() {
        return rule.name().to_string();
    }
    match rule.kind() {
        RuleKind::Weekly(_) => "Weekly schedule change",
        RuleKind::ResetWeekly => "Reset weekly schedule",
        RuleKind::Override(_) => "Day(s) override",
        RuleKind::Holiday => off,
    }
    .to_string()
}

/// When a rule starts, with when it ends if it does
fn when<V>(rule: &Rule<V>) -> String {
    if *rule.id() == DEFAULT_RULE {
        return String::new();
    }
    match rule.stop_day() {
        Some(stop) => format!(
            "{} to {}",
            format::day(rule.start_day()),
            format::day(stop.pred_opt().unwrap_or(stop))
        ),
        None => format!("from {}", format::day(rule.start_day())),
    }
}

// for_day {{{
/// The rules to list for a day, latest first and the default week last. Only the latest change of
/// weekly schedule is listed, the earlier ones are replaced by it.
fn for_day<V: Value>(set: &RuleSet<V>, day: Option<NaiveDate>) -> Vec<&Rule<V>> {
    let weekly = |r: &Rule<V>| matches!(r.kind(), RuleKind::Weekly(_) | RuleKind::ResetWeekly);
    let mut rules: Vec<&Rule<V>> = match day {
        Some(day) => set.active_rules(day),
        None => set.rules().get(&DEFAULT_RULE).into_iter().collect(),
    };
    let latest_weekly = rules
        .iter()
        .filter(|r| weekly(r) && *r.id() != DEFAULT_RULE)
        .map(|r| (r.start_day(), *r.id()))
        .max();
    rules.retain(|r| {
        !weekly(r) || *r.id() == DEFAULT_RULE || Some((r.start_day(), *r.id())) == latest_weekly
    });
    rules.sort_by_key(|r| {
        (
            *r.id() == DEFAULT_RULE,
            std::cmp::Reverse(r.start_day()),
            *r.id(),
        )
    });
    rules
}
// }}}
// }}}

// Value supporting types {{{
/// The fields of a rule being written, before it is saved
#[derive(Clone, Copy)]
struct Draft {
    /// The rule being changed, `None` for a new one
    rule: Option<Uuid>,
    /// The day the rule starts on, which is the day that was picked
    start: NaiveDate,
    kind: RwSignal<String>,
    name: RwSignal<String>,
    week: RwSignal<[u8; 7]>,
    days: RwSignal<String>,
    level: RwSignal<u8>,
    repeats: RwSignal<bool>,
    every: RwSignal<String>,
    unit: RwSignal<String>,
    from_end: RwSignal<bool>,
    until_on: RwSignal<bool>,
    until: RwSignal<NaiveDate>,
}
// }}}

// impl Draft {{{
impl Draft {
    fn new<V: Value>(day: NaiveDate, existing: Option<&Rule<V>>) -> Self {
        let start = existing.map_or(day, Rule::start_day);
        let draft = Self {
            rule: existing.map(|r| *r.id()),
            start,
            kind: RwSignal::new("override".to_string()),
            name: RwSignal::new(existing.map_or(String::new(), |r| r.name().to_string())),
            week: RwSignal::new(V::default_week().map(|v| v.level())),
            days: RwSignal::new("1".to_string()),
            level: RwSignal::new(0),
            repeats: RwSignal::new(false),
            every: RwSignal::new("1".to_string()),
            unit: RwSignal::new("week".to_string()),
            from_end: RwSignal::new(false),
            until_on: RwSignal::new(false),
            until: RwSignal::new(start),
        };
        let Some(rule) = existing else {
            return draft;
        };
        if let Some(stop) = rule.stop_day() {
            draft.until_on.set(true);
            draft.until.set(stop.pred_opt().unwrap_or(stop));
        }
        match rule.kind() {
            RuleKind::Weekly(w) => {
                draft.kind.set("weekly".to_string());
                draft
                    .week
                    .set(std::array::from_fn(|i| w.on(weekday(i)).level()));
            }
            RuleKind::Holiday => draft.kind.set("holiday".to_string()),
            RuleKind::ResetWeekly => draft.kind.set("reset".to_string()),
            RuleKind::Override(o) => {
                draft.days.set(o.days.to_string());
                draft.level.set(o.value.level());
                if let Some(e) = o.every {
                    draft.repeats.set(true);
                    draft.every.set(e.n.to_string());
                    draft.unit.set(unit_name(e.unit).to_string());
                    draft.from_end.set(e.from_end);
                }
            }
        }
        draft
    }

    fn number(text: RwSignal<String>) -> u32 {
        text.get_untracked().trim().parse().unwrap_or(1).max(1)
    }

    fn update<V: Value>(&self) -> RuleSetUpdate<V> {
        let until = self.until.get_untracked().max(self.start);
        let repeats = self.repeats.get_untracked();
        let name: String = self
            .name
            .get_untracked()
            .trim()
            .chars()
            .take(MAX_NAME)
            .collect();
        let (kind, stop) = match self.kind.get_untracked().as_str() {
            "holiday" => (RuleKind::Holiday, Some(until + Days::new(1))),
            "reset" => (RuleKind::ResetWeekly, None),
            "weekly" => (
                RuleKind::Weekly(WeeklySchedule::new(
                    self.week.get_untracked().map(V::from_level),
                )),
                None,
            ),
            _ => {
                let unit = self.unit.get_untracked();
                let every = repeats.then(|| Every {
                    n: Self::number(self.every),
                    unit: UNITS
                        .iter()
                        .find(|u| u.1 == unit)
                        .map_or(Unit::Day, |u| u.0),
                    from_end: self.from_end.get_untracked(),
                });
                let stop = (repeats && self.until_on.get_untracked()).then(|| until + Days::new(1));
                let value = V::from_level(self.level.get_untracked());
                (
                    RuleKind::Override(DayOverride {
                        days: Self::number(self.days),
                        value,
                        every,
                    }),
                    stop,
                )
            }
        };
        RuleSetUpdate::Rule(match self.rule {
            Some(id) => Crud::Update(RulePatch {
                id,
                start_day: Some(self.start),
                stop_day: Some(stop),
                kind: Some(kind),
                name: Some(name),
            }),
            None => Crud::Add(NewRule {
                id: Uuid::nil(),
                start_day: self.start,
                stop_day: stop,
                kind,
                name,
            }),
        })
    }
}
// }}}

// helpers {{{
/// Every month has room for six rows of weeks, so months sit at a fixed height and only the ones
/// in view need drawing
const MONTH_HEIGHT: f64 = 310.0;
/// How many months past what could matter the calendar keeps going
const SLACK: i32 = 6;

fn month_index(day: NaiveDate) -> i32 {
    day.year() * 12 + day.month0() as i32
}

fn month_start(index: i32) -> NaiveDate {
    NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1)
        .unwrap_or(NaiveDate::MIN)
}
// }}}

// RuleCalendar {{{
/// The calendar and the pane beside it. `send` makes a change to the rules.
#[component]
pub fn RuleCalendar<V: Value>(
    #[prop(into)] rules: Signal<Option<RuleSet<V>>>,
    send: Callback<RuleSetUpdate<V>>,
) -> impl IntoView {
    // a draft outlives the part of the page that was showing when it was made
    let owner = StoredValue::new(Owner::current());
    let make = move |day, rule: Option<&Rule<V>>| match owner.get_value() {
        Some(owner) => owner.with(|| Draft::new::<V>(day, rule)),
        None => Draft::new::<V>(day, rule),
    };
    let week_start = use_preferences().week_start;
    let today = use_today();
    let selected = RwSignal::new(None::<NaiveDate>);
    let editing = RwSignal::new(None::<Draft>);
    let scroller = NodeRef::<html::Div>::new();
    let scrolled = RwSignal::new(0.0);
    let height = RwSignal::new(600.0);
    // from a little before the earliest day that matters: the default schedule starts long before
    // any of that so it is left out
    let first = Memo::new(move |_| {
        let earliest = rules.with(|set| {
            set.as_ref()
                .into_iter()
                .flat_map(|s| s.rules().values())
                .filter(|r| *r.id() != DEFAULT_RULE)
                .map(Rule::start_day)
                .min()
        });
        earliest.map_or(month_index(today.get()), |e| {
            month_index(e.min(today.get()))
        }) - SLACK / 3
    });
    // the calendar goes on past the last rule and past wherever has been scrolled to, and shrinks
    // back when scrolled back
    let seen = move || ((scrolled.get() + height.get()) / MONTH_HEIGHT) as i32 + first.get();
    let last = Memo::new(move |_| {
        let latest = rules.with(|set| {
            set.as_ref()
                .into_iter()
                .flat_map(|s| s.rules().values())
                .filter(|r| *r.id() != DEFAULT_RULE)
                .map(|r| r.stop_day().unwrap_or(r.start_day()))
                .max()
        });
        let wanted = latest.map_or(0, month_index).max(month_index(today.get())) + 12;
        wanted.max(seen() + SLACK)
    });
    let in_view = Memo::new(move |_| {
        let top = (scrolled.get() / MONTH_HEIGHT).floor() as i32 + first.get();
        let bottom = ((scrolled.get() + height.get()) / MONTH_HEIGHT).floor() as i32 + first.get();
        (top.max(first.get()), bottom.min(last.get()))
    });
    // what every shown day is worth
    let levels = Memo::new(move |_| {
        let Some(set) = rules.get() else {
            return HashMap::new();
        };
        let (top, bottom) = in_view.get();
        let (from, to) = (month_start(top - 1), month_start(bottom + 2));
        let calendar = set.calendar(from, to);
        from.iter_days()
            .take_while(|d| *d <= to)
            .filter_map(|d| Some((d, calendar.on(d)?.level())))
            .collect::<HashMap<_, _>>()
    });
    // the day each rule was made on
    let marked = Memo::new(move |_| {
        rules.with(|set| {
            set.iter()
                .flat_map(|s| s.rules().values())
                .filter(|r| *r.id() != DEFAULT_RULE)
                .map(Rule::start_day)
                .collect::<std::collections::HashSet<_>>()
        })
    });
    // the rules to list, latest first
    let listed = Memo::new(move |_| {
        let Some(set) = rules.get() else {
            return Vec::new();
        };
        for_day(&set, selected.get())
            .into_iter()
            .map(|r| (*r.id(), title(r, V::off_text()), when(r)))
            .collect::<Vec<_>>()
    });
    let measure = move || {
        if let Some(element) = scroller.get_untracked() {
            scrolled.set(f64::from(element.scroll_top()));
            height.set(f64::from(element.client_height()));
        }
    };
    let to_today = move || {
        if let Some(element) = scroller.get_untracked() {
            let at = f64::from(month_index(today.get_untracked()) - first.get_untracked())
                * MONTH_HEIGHT;
            element.set_scroll_top(at as i32);
            measure();
        }
    };
    Effect::new(move |_| {
        if scroller.get().is_some() {
            to_today();
        }
    });
    let edit = move |id: Uuid| {
        let day = selected.get_untracked().unwrap_or(today.get_untracked());
        let draft = rules.with_untracked(|set| {
            set.as_ref()
                .and_then(|s| s.rules().get(&id))
                .map(|rule| make(day, Some(rule)))
        });
        editing.set(draft);
    };
    view! {
        <div class="rulecal">
            <div class="calcol">
                <div class="calbar">
                    <button type="button" on:click=move |_| {
                        to_today();
                        selected.set(Some(today.get_untracked()));
                        editing.set(None);
                    }>"Today"</button>
                </div>
                <div class="months" node_ref=scroller on:scroll=move |_| measure()>
                    <div class="monthspace" style=move || format!("height:{}px", f64::from(last.get() - first.get() + 1) * MONTH_HEIGHT)>
                        <For
                            each=move || { let (top, bottom) = in_view.get(); top..=bottom }
                            key=|index| *index
                            children=move |index| {
                                let month = month_start(index);
                                let next = month_start(index + 1);
                                let blanks = move || week_start.get().offset(month);
                                let days: Vec<NaiveDate> = month.iter_days().take_while(|d| *d < next).collect();
                                view! {
                                    <section class="month" style=move || format!("top:{}px", f64::from(index - first.get()) * MONTH_HEIGHT)>
                                        <h3>{format!("{} {}", format::month(month), month.year())}</h3>
                                        <div class="days">
                                            {move || week_start.get().weekdays().into_iter().map(|n| view! { <span class="head">{n}</span> }).collect_view()}
                                            {move || (0..blanks()).map(|_| view! { <span></span> }).collect_view()}
                                            {days.into_iter().map(|day| day_cell::<V>(day, today, levels, marked, selected, editing)).collect_view()}
                                        </div>
                                    </section>
                                }
                            }
                        />
                    </div>
                </div>
            </div>
            <aside class="rulepane">
                    <div class="rule-editor">
                        {move || match editing.get() {
                            None => view! {
                                <p class="small">
                                    {move || if selected.get().is_some() {
                                        "Pick a rule below to change it, or add a new one."
                                    } else {
                                        "Pick a day to add a rule on it, or change the default week below."
                                    }}
                                </p>
                                <Show when=move || selected.get().is_some()>
                                    <button type="button" class="add" on:click=move |_| {
                                        let day = selected.get_untracked().unwrap_or(today.get_untracked());
                                        editing.set(Some(make(day, None)));
                                    }>"+ New rule"</button>
                                </Show>
                            }.into_any(),
                            Some(draft) => view! {
                                <RuleForm
                                    draft=draft
                                    max=V::MAX
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
                        <h3>{move || selected.get().map_or("Default".to_string(), format::day)}</h3>
                        <Show when=move || editing.with(|e| e.as_ref().is_some_and(|d| d.rule.is_none()))>
                            <div class="rule draft"><span>"New rule (not saved yet)"</span></div>
                        </Show>
                        <For each=move || listed.get() key=|r| format!("{r:?}") children=move |(id, name, from)| view! {
                            <div class="rule" class:picked=move || editing.with(|e| e.as_ref().is_some_and(|d| d.rule == Some(id)))>
                                <span><strong>{name}</strong><br /><span class="small">{from}</span></span>
                                <button type="button" class="plain" on:click=move |_| edit(id)>"Edit"</button>
                                <Show when=move || id != DEFAULT_RULE>
                                    <button type="button" class="plain" aria-label="Delete rule" on:click=move |_| {
                                        send.run(RuleSetUpdate::Rule(Crud::Remove(id)));
                                        editing.update(|e| if e.as_ref().is_some_and(|d| d.rule == Some(id)) { *e = None });
                                    }>"✕"</button>
                                </Show>
                            </div>
                        } />
                    </div>
            </aside>
        </div>
    }
}
// }}}

// day_cell {{{
/// A day of the calendar: shaded by what it is worth, with a dot on the day a rule was made
fn day_cell<V: Value>(
    day: NaiveDate,
    today: Memo<NaiveDate>,
    levels: Memo<HashMap<NaiveDate, u8>>,
    marked: Memo<std::collections::HashSet<NaiveDate>>,
    selected: RwSignal<Option<NaiveDate>>,
    editing: RwSignal<Option<Draft>>,
) -> impl IntoView {
    // each day only changes when its own value does, not whenever any day's does
    let level = Memo::new(move |_| levels.with(|l| l.get(&day).copied()).unwrap_or(0));
    let picked = Memo::new(move |_| selected.get() == Some(day));
    let mark = Memo::new(move |_| marked.with(|m| m.contains(&day)));
    view! {
        <button
            type="button"
            class="day"
            style=move || format!("--shade:{}", f32::from(level.get()) / f32::from(V::MAX))
            class:today=move || day == today.get()
            class:picked=move || picked.get()
            class:marked=move || mark.get()
            title=move || format!("{}: {}", format::day(day), V::level_text(level.get()))
            on:click=move |_| { selected.set(Some(day)); editing.set(None); }
        >
            {day.day()}
        </button>
    }
}
// }}}

// RuleForm {{{
#[component]
fn RuleForm(
    draft: Draft,
    /// The most a day can be given
    max: u8,
    /// What a rule that takes days away is called
    off: &'static str,
    on_save: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let kind = draft.kind;
    let default = draft.rule == Some(DEFAULT_RULE);
    let level = Signal::derive(move || i64::from(draft.level.get()));
    view! {
        <form class="form" on:submit=move |ev| { ev.prevent_default(); on_save.run(()); }>
            <Show when=move || !default>
                <Field label="What">
                    <Select
                        value=kind
                        options=options([
                            ("override", "Day(s) override"),
                            ("weekly", "Weekly schedule change"),
                            ("reset", "Reset weekly schedule"),
                            ("holiday", off),
                        ])
                        on_change=Callback::new(move |v| kind.set(v))
                    />
                </Field>
                <Field label="Name (optional)">
                    <input
                        type="text"
                        maxlength=MAX_NAME.to_string()
                        placeholder="Shown instead of the kind of rule"
                        prop:value=move || draft.name.get()
                        on:input=move |ev| draft.name.set(event_target_value(&ev))
                    />
                </Field>
                <p class="small">{format!("Starts {}", format::day(draft.start))}</p>
            </Show>
            <Show when=move || kind.get() == "weekly">
                <div class="weekrow">
                    {(0..7).map(|i| view! {
                        <div class="field">
                            <span>{format::WEEKDAYS[i]}</span>
                            <Hours
                                max=max
                                value=Signal::derive(move || draft.week.with(|w| i64::from(w[i])))
                                on_change=Callback::new(move |l: u8| draft.week.update(|w| w[i] = l))
                            />
                        </div>
                    }).collect_view()}
                </div>
            </Show>
            <Show when=move || kind.get() == "override">
                <Field label="Number of days">
                    <input
                        type="number"
                        min="1"
                        prop:value=move || draft.days.get()
                        on:input=move |ev| draft.days.set(event_target_value(&ev))
                    />
                </Field>
                <div class="field">
                    <span>"Value"</span>
                    <Hours max=max value=level on_change=Callback::new(move |l| draft.level.set(l)) />
                </div>
                <label class="check">
                    <input type="checkbox" prop:checked=move || draft.repeats.get() on:change=move |ev| draft.repeats.set(event_target_checked(&ev)) />
                    "Recurring"
                </label>
                <Show when=move || draft.repeats.get()>
                    <div class="row">
                        <span>"Every:"</span>
                        <input
                            type="number"
                            min="1"
                            prop:value=move || draft.every.get()
                            on:input=move |ev| draft.every.set(event_target_value(&ev))
                        />
                        <Select
                            value=draft.unit
                            options=Signal::stored(UNITS.iter().map(|u| (u.1.to_string(), u.2.to_string())).collect::<Vec<_>>())
                            on_change=Callback::new(move |v| draft.unit.set(v))
                        />
                    </div>
                    <Show when=move || draft.unit.get() == "aligned">
                        <label class="check" title="Counts the weekday from the end of the month, so the last Monday and not the fourth">
                            <input type="checkbox" prop:checked=move || draft.from_end.get() on:change=move |ev| draft.from_end.set(event_target_checked(&ev)) />
                            "Align from month end"
                        </label>
                    </Show>
                    <label class="check">
                        <input type="checkbox" prop:checked=move || draft.until_on.get() on:change=move |ev| draft.until_on.set(event_target_checked(&ev)) />
                        "Until"
                    </label>
                    <Show when=move || draft.until_on.get()>
                        <DateInput day=draft.until />
                    </Show>
                </Show>
            </Show>
            <Show when=move || kind.get() == "holiday">
                <Field label="Last day"><DateInput day=draft.until /></Field>
            </Show>
            <div class="actions">
                <span class="grow"></span>
                <button type="button" on:click=move |_| on_cancel.run(())>"Cancel"</button>
                <button type="submit" class="primary">"Save"</button>
            </div>
        </form>
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn set_with(rules: &[(u32, RuleKind<Workload>)]) -> (RuleSet<Workload>, Vec<Uuid>) {
        let mut set = RuleSet::<Workload>::default();
        let mut ids = Vec::new();
        for (day, kind) in rules {
            let id = Uuid::new_v4();
            let new = NewRule {
                name: String::new(),
                id,
                start_day: d(*day),
                stop_day: None,
                kind: kind.clone(),
            };
            set.apply_update(RuleSetUpdate::Rule(Crud::Add(new)))
                .unwrap();
            ids.push(id);
        }
        (set, ids)
    }

    #[test]
    fn rules_are_named_by_their_kind_unless_given_a_name() {
        let hours = Workload::new_const(4);
        let once = DayOverride {
            days: 3,
            value: hours,
            every: None,
        };
        let (set, ids) = set_with(&[(8, RuleKind::Override(once)), (9, RuleKind::ResetWeekly)]);
        let rule = |i: usize| &set.rules()[&ids[i]];
        assert_eq!(title(rule(0), "Time off"), "Day(s) override");
        assert_eq!(when(rule(0)), "from Mon 8 Jan");
        assert_eq!(title(rule(1), "Time off"), "Reset weekly schedule");
        assert_eq!(
            title(&set.rules()[&DEFAULT_RULE], "Time off"),
            "Default week"
        );
        assert_eq!(when(&set.rules()[&DEFAULT_RULE]), "");
        let patch = RulePatch {
            id: ids[0],
            start_day: None,
            stop_day: None,
            kind: None,
            name: Some("Dentist".to_string()),
        };
        let mut set = set;
        set.apply_update(RuleSetUpdate::Rule(Crud::Update(patch)))
            .unwrap();
        assert_eq!(title(&set.rules()[&ids[0]], "Time off"), "Dentist");
    }

    #[test]
    fn a_day_lists_latest_first_with_only_the_latest_weekly_change_and_the_default_last() {
        let week = |h| RuleKind::Weekly(WeeklySchedule::from_hours([h; 7]).unwrap());
        let hours = Workload::new_const(4);
        let once = RuleKind::Override(DayOverride {
            days: 30,
            value: hours,
            every: None,
        });
        let (set, ids) = set_with(&[
            (2, week(1)),
            (3, once),
            (4, week(2)),
            (5, RuleKind::ResetWeekly),
        ]);
        let listed: Vec<Uuid> = for_day(&set, Some(d(10))).iter().map(|r| *r.id()).collect();
        // the reset is the latest weekly change, so the two before it are left out
        assert_eq!(listed, [ids[3], ids[1], DEFAULT_RULE]);
        let before: Vec<Uuid> = for_day(&set, Some(d(4))).iter().map(|r| *r.id()).collect();
        assert_eq!(before, [ids[2], ids[1], DEFAULT_RULE]);
        let none: Vec<Uuid> = for_day(&set, None).iter().map(|r| *r.id()).collect();
        assert_eq!(none, [DEFAULT_RULE]);
    }

    #[test]
    fn a_draft_makes_the_rule_it_was_made_from() {
        let week = WeeklySchedule::from_hours([8, 8, 8, 8, 4, 0, 0]).unwrap();
        let mut set = RuleSet::<Workload>::default();
        let id = Uuid::from_u128(1);
        let new = NewRule {
            name: String::new(),
            id,
            start_day: d(8),
            stop_day: None,
            kind: RuleKind::Weekly(week.clone()),
        };
        set.apply_update(RuleSetUpdate::Rule(Crud::Add(new)))
            .unwrap();
        let draft = Draft::new(d(1), Some(&set.rules()[&id]));
        let RuleSetUpdate::Rule(Crud::Update(patch)) = draft.update::<Workload>() else {
            panic!("a patch");
        };
        assert_eq!(patch.kind, Some(RuleKind::Weekly(week)));
        assert_eq!(patch.start_day, Some(d(8)));
    }

    #[test]
    fn a_new_draft_is_one_day_that_does_not_repeat() {
        let draft = Draft::new::<Workload>(d(5), None);
        let RuleSetUpdate::Rule(Crud::Add(new)) = draft.update::<Workload>() else {
            panic!("an addition");
        };
        assert_eq!(new.start_day, d(5));
        assert_eq!(new.stop_day, None);
        assert!(matches!(
            new.kind,
            RuleKind::Override(DayOverride {
                days: 1,
                every: None,
                ..
            })
        ));
        // a holiday always ends, here the day it starts
        draft.kind.set("holiday".to_string());
        let RuleSetUpdate::Rule(Crud::Add(new)) = draft.update::<Workload>() else {
            panic!("an addition");
        };
        assert_eq!(new.stop_day, Some(d(6)));
    }
}
// }}}
