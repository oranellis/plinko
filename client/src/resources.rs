//! How booked everyone is, day by day, and who the bottlenecks are.

use chrono::{Datelike, Days, NaiveDate};
use leptos::prelude::*;
use uuid::Uuid;

use crate::{
    format,
    nav::{Availability, Route},
    ui::use_sync,
};

const WEEKS: u64 = 4;

#[derive(Clone, PartialEq)]
struct Person {
    id: Uuid,
    name: String,
    load: f32,
    /// Booked and available hours for each day shown
    days: Vec<(u32, u8)>,
}

fn heat(booked: u32, capacity: u8) -> &'static str {
    match (booked, capacity) {
        (0, 0) => "off",
        (_, 0) => "heat-4",
        (0, _) => "heat-0",
        (b, c) => {
            let ratio = f64::from(b) / f64::from(c);
            if ratio < 0.5 {
                "heat-1"
            } else if ratio < 0.85 {
                "heat-2"
            } else if ratio <= 1.0 {
                "heat-3"
            } else {
                "heat-4"
            }
        }
    }
}

#[component]
pub fn Resources() -> impl IntoView {
    let sync = use_sync();
    let offset = RwSignal::new(0i64);
    let first = Memo::new(move |_| {
        let today = sync.tenant.with(|t| t.as_ref().map(|t| t.today()))?;
        let monday = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
        let shift = Days::new(offset.get().unsigned_abs() * 7);
        Some(if offset.get() < 0 {
            monday - shift
        } else {
            monday + shift
        })
    });
    let people = Memo::new(move |_| {
        let first = first.get()?;
        let last = first + Days::new(WEEKS * 7 - 1);
        let schedule = sync.schedule.get();
        sync.tenant.with(|t| {
            let t = t.as_ref()?;
            let mut people: Vec<Person> = t
                .users()
                .iter()
                .map(|(id, user)| {
                    let calendar = user.availability().calendar(first, last);
                    let booked = schedule.as_ref().and_then(|(_, s)| s.users.get(id));
                    Person {
                        id: *id,
                        name: user.name().to_string(),
                        load: booked.map_or(0.0, |b| b.bottleneck),
                        days: (0..WEEKS * 7)
                            .map(|d| {
                                let day = first + Days::new(d);
                                (
                                    booked.map_or(0, |b| b.booked_on(day)),
                                    calendar.on(day).map_or(0, |h| h.get()),
                                )
                            })
                            .collect(),
                    }
                })
                .collect();
            people.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
            Some(people)
        })
    });
    let header = move || {
        let first = first.get()?;
        Some(
            (0..WEEKS * 7)
                .map(|d| first + Days::new(d))
                .collect::<Vec<NaiveDate>>(),
        )
    };
    view! {
        <div class="bar">
            <h1>"Resources"</h1>
            <button on:click=move |_| offset.update(|o| *o -= 1)>"← Earlier"</button>
            <button on:click=move |_| offset.set(0)>"This week"</button>
            <button on:click=move |_| offset.update(|o| *o += 1)>"Later →"</button>
        </div>
        <p class="small">"Hours booked of the hours each person has. Red is more than they have."</p>
        <div class="scroll">
            <table class="grid">
                <thead>
                    <tr>
                        <th>"Person"</th>
                        {move || header().unwrap_or_default().into_iter().map(|d| view! {
                            <th class:off=matches!(d.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun)>
                                {format!("{} {}", format::weekday(d), d.day())}
                            </th>
                        }).collect_view()}
                    </tr>
                </thead>
                <tbody>
                    {move || people.get().unwrap_or_default().into_iter().map(|p| view! {
                        <tr>
                            <td>
                                <strong>{p.name.clone()}</strong>
                                {(p.load > 0.0).then(|| view! { <span class="small">{format!(" {:.0}% booked", p.load * 100.0)}</span> })}
                                " "
                                <a href=Route::Availability(Availability::User(p.id)).path()>"Hours"</a>
                            </td>
                            {p.days.iter().map(|(booked, capacity)| view! {
                                <td class=heat(*booked, *capacity)>
                                    {if *booked == 0 && *capacity == 0 { String::new() } else { format!("{booked}/{capacity}") }}
                                </td>
                            }).collect_view()}
                        </tr>
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_coloured_by_how_full_they_are() {
        assert_eq!(heat(0, 0), "off");
        assert_eq!(heat(3, 0), "heat-4");
        assert_eq!(heat(0, 8), "heat-0");
        assert_eq!(heat(2, 8), "heat-1");
        assert_eq!(heat(6, 8), "heat-2");
        assert_eq!(heat(8, 8), "heat-3");
        assert_eq!(heat(9, 8), "heat-4");
    }
}
