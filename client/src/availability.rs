//! Editing when people work: the rules for a person, or for the placeholders of a plan. The
//! rules are the ones the scheduler uses, so the calendar shown is what it will schedule against.

use chrono::{Datelike, Days, NaiveDate, Weekday};
use leptos::prelude::*;
use plinko::{
    availability::{
        rule::{MonthTarget, MonthlyOverride, NewRule, Ordinal, Rule, RuleKind, WeeklySchedule},
        rule_set::{RuleSet, RuleSetUpdate},
    },
    multitenancy::tenant_update::TenantUpdateType,
    planner::plan_update::PlanUpdate,
    types::{crud::Crud, workload::Workload},
};
use uuid::Uuid;

use crate::{
    datepicker::DateInput,
    format,
    nav::{Availability as Of, Route},
    ui::{Field, Number, Select, options, use_sync},
};

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

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

fn weekday_name(day: Weekday) -> &'static str {
    DAYS[day.num_days_from_monday() as usize]
}

/// A rule in words
pub fn describe(rule: &Rule<Workload>) -> String {
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
            let hours: Vec<String> = DAYS
                .iter()
                .zip(0..7)
                .map(|(name, i)| {
                    let day = NaiveDate::from_ymd_opt(2024, 1, 1 + i).expect("a date");
                    format!("{name} {}", week.on(day.weekday()).get())
                })
                .collect();
            format!("Weekly hours: {}", hours.join(", "))
        }
        RuleKind::Holiday => "Time off".to_string(),
        RuleKind::FiniteWorkdays { days, value } => {
            format!("The next {days} working days: {} h", value.get())
        }
        RuleKind::Monthly(m) => {
            let target = match m.target {
                MonthTarget::Day => "day".to_string(),
                MonthTarget::Weekday(day) => weekday_name(day).to_string(),
                MonthTarget::Week(_) => "week".to_string(),
            };
            format!(
                "Every {} {target} of the month: {} h",
                ordinal_name(m.ordinal),
                m.value.get()
            )
        }
    };
    format!("{what}, {span}")
}

#[component]
pub fn Availability(of: Of) -> impl IntoView {
    let sync = use_sync();
    let rules = Memo::new(move |_| {
        sync.tenant.with(|t| {
            let t = t.as_ref()?;
            match of {
                Of::User(id) => t.users().get(&id).map(|u| u.availability().clone()),
                Of::Plan(id) => t.plans().get(&id).map(|p| p.default_availability().clone()),
            }
        })
    });
    let title = move || {
        sync.tenant
            .with(|t| {
                let t = t.as_ref()?;
                Some(match of {
                    Of::User(id) => format!("Hours of {}", t.users().get(&id)?.name()),
                    Of::Plan(id) => {
                        format!("Hours of placeholders in {}", t.plans().get(&id)?.name())
                    }
                })
            })
            .unwrap_or_default()
    };
    let send = move |update: RuleSetUpdate<Workload>| match of {
        Of::User(id) => sync.send(TenantUpdateType::Availability(id, update)),
        Of::Plan(id) => sync.plan(id, PlanUpdate::Availability(update)),
    };
    let back = match of {
        Of::User(_) => Route::Resources,
        Of::Plan(id) => Route::Plan(id, crate::nav::PlanTab::Settings, None),
    };
    view! {
        <div class="people">
            <div class="bar">
                <h1>{title}</h1>
                <a href=back.path()>"Back"</a>
            </div>
            <Show when=move || rules.with(Option::is_some) fallback=|| view! { <p>"Not found."</p> }>
                <section>
                    <h2>"The calendar this makes"</h2>
                    <Preview rules=rules />
                </section>
                <section>
                    <h2>"Rules"</h2>
                    <p class="small">"Later rules replace earlier ones from their start day on."</p>
                    <RuleList rules=rules on_remove=Callback::new(move |id| send(RuleSetUpdate::Rule(Crud::Remove(id)))) />
                </section>
                <AddRule on_add=Callback::new(move |rule| send(RuleSetUpdate::Rule(Crud::Add(rule)))) />
            </Show>
        </div>
    }
}

#[component]
fn Preview(rules: Memo<Option<RuleSet<Workload>>>) -> impl IntoView {
    let sync = use_sync();
    let weeks = move || {
        let today = sync
            .tenant
            .with(|t| t.as_ref().map(|t| t.today()))
            .unwrap_or_default();
        let monday = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
        let Some(set) = rules.get() else {
            return Vec::new();
        };
        let calendar = set.calendar(monday, monday + Days::new(7 * 6 - 1));
        (0..6)
            .map(|week| {
                let start = monday + Days::new(week * 7);
                let days: Vec<(NaiveDate, u8)> = (0..7)
                    .map(|d| {
                        let day = start + Days::new(d);
                        (day, calendar.on(day).map_or(0, |h| h.get()))
                    })
                    .collect();
                (start, days)
            })
            .collect::<Vec<_>>()
    };
    view! {
        <div class="scroll">
            <table class="grid">
                <thead>
                    <tr><th>"Week of"</th>{DAYS.iter().map(|d| view! { <th>{*d}</th> }).collect_view()}</tr>
                </thead>
                <tbody>
                    {move || weeks().into_iter().map(|(start, days)| view! {
                        <tr>
                            <td>{format::day(start)}</td>
                            {days.into_iter().map(|(day, hours)| view! {
                                <td class=if hours == 0 { "off" } else { "heat-1" } title=format::day(day)>{hours}</td>
                            }).collect_view()}
                        </tr>
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn RuleList(rules: Memo<Option<RuleSet<Workload>>>, on_remove: Callback<Uuid>) -> impl IntoView {
    let listed = move || {
        let Some(set) = rules.get() else {
            return Vec::new();
        };
        let mut all: Vec<(Uuid, NaiveDate, String)> = set
            .rules()
            .values()
            .map(|r| (*r.id(), r.start_day(), describe(r)))
            .collect();
        all.sort_by_key(|(id, start, _)| (*start, *id));
        all
    };
    view! {
        <ul class="items">
            {move || listed().into_iter().map(|(id, _, text)| view! {
                <li class="row">
                    <span>{text}</span>
                    <button type="button" class="plain" aria-label="Remove rule" on:click=move |_| on_remove.run(id)>"✕"</button>
                </li>
            }).collect_view()}
        </ul>
    }
}

#[component]
fn AddRule(on_add: Callback<NewRule<Workload>>) -> impl IntoView {
    let sync = use_sync();
    let today = sync
        .tenant
        .with_untracked(|t| t.as_ref().map(|t| t.today()))
        .unwrap_or_default();
    let kind = RwSignal::new("weekly".to_string());
    let start = RwSignal::new(today);
    let until = RwSignal::new(today + Days::new(7));
    let ends = RwSignal::new(false);
    let week = RwSignal::new([8u8, 8, 8, 8, 8, 0, 0]);
    let count = RwSignal::new(5i64);
    let hours = RwSignal::new(4i64);
    let ordinal = RwSignal::new("first".to_string());
    let weekday = RwSignal::new(0usize);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let hour = |h: i64| Workload::new(h.clamp(0, 8) as u8).expect("clamped to a workload");
        let kind = match kind.get_untracked().as_str() {
            "holiday" => RuleKind::Holiday,
            "days" => RuleKind::FiniteWorkdays {
                days: count.get_untracked().clamp(1, 3650) as u32,
                value: hour(hours.get_untracked()),
            },
            "monthly" => {
                let ordinal = [
                    Ordinal::First,
                    Ordinal::Second,
                    Ordinal::Third,
                    Ordinal::Fourth,
                    Ordinal::Fifth,
                    Ordinal::Last,
                    Ordinal::SecondLast,
                ]
                .into_iter()
                .find(|o| ordinal_name(*o) == ordinal.get_untracked())
                .unwrap_or(Ordinal::First);
                let day = NaiveDate::from_ymd_opt(2024, 1, 1 + weekday.get_untracked() as u32)
                    .expect("a date");
                RuleKind::Monthly(MonthlyOverride {
                    ordinal,
                    target: MonthTarget::Weekday(day.weekday()),
                    value: hour(hours.get_untracked()),
                    priority: 0,
                })
            }
            _ => RuleKind::Weekly(WeeklySchedule::new(
                week.get_untracked().map(|h| hour(i64::from(h))),
            )),
        };
        let stop_day = (ends.get_untracked() && start.get_untracked() < until.get_untracked())
            .then(|| until.get_untracked() + Days::new(1));
        on_add.run(NewRule {
            id: Uuid::nil(),
            start_day: start.get_untracked(),
            stop_day,
            kind,
        });
    };
    let hour_options = options([
        ("0", "0 h"),
        ("1", "1 h"),
        ("2", "2 h"),
        ("3", "3 h"),
        ("4", "4 h"),
        ("5", "5 h"),
        ("6", "6 h"),
        ("7", "7 h"),
        ("8", "8 h"),
    ]);
    view! {
        <section>
            <h2>"Add a rule"</h2>
            <form class="form" on:submit=submit>
                <Field label="What">
                    <Select
                        value=kind
                        options=options([
                            ("weekly", "Weekly hours"),
                            ("holiday", "Time off"),
                            ("days", "Set the next working days"),
                            ("monthly", "A day every month"),
                        ])
                        on_change=Callback::new(move |v| kind.set(v))
                    />
                </Field>
                <Field label="From"><DateInput day=start /></Field>
                <Show when=move || kind.get() != "weekly">
                    <label class="check">
                        <input type="checkbox" prop:checked=move || ends.get() on:change=move |ev| ends.set(event_target_checked(&ev)) />
                        "Until a day"
                    </label>
                    <Show when=move || ends.get()><Field label="Last day"><DateInput day=until /></Field></Show>
                </Show>
                <Show when=move || kind.get() == "weekly">
                    <div class="row">
                        {DAYS.iter().enumerate().map(|(i, name)| view! {
                            <Field label=*name>
                                <Select
                                    value=Signal::derive(move || week.with(|w| w[i].to_string()))
                                    options=hour_options
                                    on_change=Callback::new(move |v: String| {
                                        if let Ok(h) = v.parse::<u8>() { week.update(|w| w[i] = h); }
                                    })
                                />
                            </Field>
                        }).collect_view()}
                    </div>
                </Show>
                <Show when=move || kind.get() == "days">
                    <Field label="How many working days"><Number value=count on_input=Callback::new(move |n| count.set(n)) min=1 /></Field>
                </Show>
                <Show when=move || kind.get() == "monthly">
                    <div class="row">
                        <Select
                            value=ordinal
                            options=options([
                                ("first", "first"), ("second", "second"), ("third", "third"),
                                ("fourth", "fourth"), ("last", "last"),
                            ])
                            on_change=Callback::new(move |v| ordinal.set(v))
                        />
                        <Select
                            value=Signal::derive(move || weekday.get().to_string())
                            options=options([
                                ("0", "Monday"), ("1", "Tuesday"), ("2", "Wednesday"), ("3", "Thursday"),
                                ("4", "Friday"), ("5", "Saturday"), ("6", "Sunday"),
                            ])
                            on_change=Callback::new(move |v: String| {
                                if let Ok(i) = v.parse::<usize>() { weekday.set(i); }
                            })
                        />
                    </div>
                </Show>
                <Show when=move || matches!(kind.get().as_str(), "days" | "monthly")>
                    <Field label="Hours">
                        <Number value=hours on_input=Callback::new(move |n| hours.set(n)) min=0 max=8 />
                    </Field>
                </Show>
                <button type="submit" class="primary">"Add rule"</button>
            </form>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn rule(kind: RuleKind<Workload>, stop: Option<NaiveDate>) -> String {
        let mut set = RuleSet::<Workload>::default();
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
        assert_eq!(rule(RuleKind::Holiday, None), "Time off, from Mon 8 Jan");
        assert_eq!(
            rule(RuleKind::Holiday, Some(d(13))),
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
            "Weekly hours: Mon 8, Tue 8, Wed 8, Thu 8, Fri 4, Sat 0, Sun 0, from Mon 8 Jan"
        );
    }
}
