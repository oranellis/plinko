//! The icons, drawn as small inline SVG on a 16 by 16 grid in the colour of the text, the way
//! Plinko 1 drew them.

use leptos::prelude::*;

// Icon {{{
#[component]
pub fn Icon(
    /// Which icon
    kind: IconKind,
    #[prop(default = 24)] size: u32,
) -> impl IntoView {
    let body = match kind {
        IconKind::Back => view! {
            <polyline points="8.6,4.6 5.2,8 8.6,11.4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
        }
        .into_any(),
        IconKind::Gantt => view! {
            <rect x="2" y="3" width="7" height="2.5" rx="1" fill="currentColor" />
            <rect x="5" y="6.75" width="6" height="2.5" rx="1" fill="currentColor" />
            <rect x="3" y="10.5" width="8" height="2.5" rx="1" fill="currentColor" />
        }
        .into_any(),
        IconKind::List => view! {
            <line x1="5.5" y1="4" x2="14" y2="4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
            <line x1="5.5" y1="8" x2="14" y2="8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
            <line x1="5.5" y1="12" x2="14" y2="12" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
            <circle cx="2.6" cy="4" r="0.9" fill="currentColor" />
            <circle cx="2.6" cy="8" r="0.9" fill="currentColor" />
            <circle cx="2.6" cy="12" r="0.9" fill="currentColor" />
        }
        .into_any(),
        IconKind::Allocation => view! {
            <rect x="2" y="2" width="3" height="7" rx="1" fill="currentColor" />
            <rect x="6.5" y="2" width="3" height="11" rx="1" fill="currentColor" />
            <rect x="11" y="2" width="3" height="5" rx="1" fill="currentColor" />
        }
        .into_any(),
        IconKind::Settings => view! {
            <path
                d="M6.5 1.5h3l.5 1.8a5 5 0 0 1 1.2.7l1.8-.6 1.5 2.6-1.4 1.2a5 5 0 0 1 0 1.6l1.4 1.2-1.5 2.6-1.8-.6a5 5 0 0 1-1.2.7l-.5 1.8h-3l-.5-1.8A5 5 0 0 1 4.8 12l-1.8.6L1.5 10l1.4-1.2a5 5 0 0 1 0-1.6L1.5 6l1.5-2.6 1.8.6A5 5 0 0 1 6 3.3Z"
                stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"
            />
            <circle cx="8" cy="8" r="2.1" stroke="currentColor" stroke-width="1.3" />
        }
        .into_any(),
        IconKind::Resources => view! {
            <circle cx="4.5" cy="4.5" r="2" stroke="currentColor" stroke-width="1.3" />
            <path d="M1 13c0-2 1.6-3.5 3.5-3.5S8 11 8 13" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
            <line x1="10" y1="5" x2="15" y2="5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
            <line x1="10" y1="8" x2="15" y2="8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
            <line x1="10" y1="11" x2="15" y2="11" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::Users => view! {
            <circle cx="6" cy="5.5" r="2.3" stroke="currentColor" stroke-width="1.3" />
            <path d="M1.5 13.5c0-2.5 2-4 4.5-4s4.5 1.5 4.5 4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
            <circle cx="11.5" cy="5" r="1.8" stroke="currentColor" stroke-width="1.2" />
            <path d="M11 9.5c1.5 0 3 1 3 3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::Search => view! {
            <circle cx="6.5" cy="6.5" r="4" stroke="currentColor" stroke-width="0.95" />
            <line x1="9.8" y1="9.8" x2="13.5" y2="13.5" stroke="currentColor" stroke-width="1.05" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::Today => view! {
            <rect x="2" y="3" width="12" height="11" rx="1.5" stroke="currentColor" stroke-width="0.9" />
            <line x1="2" y1="7" x2="14" y2="7" stroke="currentColor" stroke-width="0.8" />
            <line x1="5.5" y1="1.5" x2="5.5" y2="5" stroke="currentColor" stroke-width="0.95" stroke-linecap="round" />
            <line x1="10.5" y1="1.5" x2="10.5" y2="5" stroke="currentColor" stroke-width="0.95" stroke-linecap="round" />
            <line x1="8" y1="9" x2="8" y2="13" stroke="currentColor" stroke-width="0.95" stroke-linecap="round" />
            <line x1="5.5" y1="11" x2="10.5" y2="11" stroke="currentColor" stroke-width="0.95" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::AddTask => view! {
            <rect x="1.5" y="4" width="9" height="2.5" rx="1" fill="currentColor" />
            <rect x="1.5" y="9.5" width="6" height="2.5" rx="1" fill="currentColor" />
            <line x1="12" y1="9" x2="12" y2="15" stroke="currentColor" stroke-width="1.05" stroke-linecap="round" />
            <line x1="9" y1="12" x2="15" y2="12" stroke="currentColor" stroke-width="1.05" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::AddMilestone => view! {
            <path d="M7 2.5L2.5 7L7 11.5L11.5 7Z" stroke="currentColor" stroke-width="0.95" stroke-linejoin="round" />
            <line x1="13" y1="10" x2="13" y2="15.5" stroke="currentColor" stroke-width="1.0" stroke-linecap="round" />
            <line x1="10.25" y1="12.75" x2="15.75" y2="12.75" stroke="currentColor" stroke-width="1.0" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::AddGroup => view! {
            <rect x="1.5" y="2.5" width="10" height="9" rx="2" stroke="currentColor" stroke-width="1" />
            <rect x="3.5" y="5" width="5" height="2" rx="0.8" fill="currentColor" />
            <line x1="13" y1="10" x2="13" y2="15.5" stroke="currentColor" stroke-width="1.0" stroke-linecap="round" />
            <line x1="10.25" y1="12.75" x2="15.75" y2="12.75" stroke="currentColor" stroke-width="1.0" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::AddBuffer => view! {
            <rect x="1.5" y="4" width="9" height="3" rx="1" stroke="currentColor" stroke-width="1" stroke-dasharray="2 1.4" />
            <line x1="13" y1="10" x2="13" y2="15.5" stroke="currentColor" stroke-width="1.0" stroke-linecap="round" />
            <line x1="10.25" y1="12.75" x2="15.75" y2="12.75" stroke="currentColor" stroke-width="1.0" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::AddDelay => view! {
            <circle cx="6.5" cy="6.5" r="4.5" stroke="currentColor" stroke-width="1" />
            <path d="M6.5 4V6.5L8.2 7.6" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" />
            <line x1="13" y1="10" x2="13" y2="15.5" stroke="currentColor" stroke-width="1.0" stroke-linecap="round" />
            <line x1="10.25" y1="12.75" x2="15.75" y2="12.75" stroke="currentColor" stroke-width="1.0" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::Circle => view! {
            <circle cx="8" cy="8" r="5.2" stroke="currentColor" stroke-width="1.4" />
        }
        .into_any(),
        IconKind::Pause => view! {
            <rect x="4.6" y="3.6" width="2.4" height="8.8" rx="0.6" fill="currentColor" />
            <rect x="9" y="3.6" width="2.4" height="8.8" rx="0.6" fill="currentColor" />
        }
        .into_any(),
        IconKind::Drop => view! {
            <circle cx="8" cy="8" r="5.2" stroke="currentColor" stroke-width="1.4" />
            <line x1="4.5" y1="11.5" x2="11.5" y2="4.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::Pin => view! {
            <path d="M8 14V9.4M5.4 3.4H10.6L10 8.4H6Z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::Reset => view! {
            <path d="M12.8 8A4.8 4.8 0 1 1 11.4 4.6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
            <polyline points="12.2,2.2 12.2,5 9.4,5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
        }
        .into_any(),
        IconKind::Data => view! {
            <rect x="2.4" y="3" width="11.2" height="3.2" rx="1" stroke="currentColor" stroke-width="1.2" />
            <rect x="2.4" y="9.8" width="11.2" height="3.2" rx="1" stroke="currentColor" stroke-width="1.2" />
            <line x1="8" y1="6.2" x2="8" y2="9.8" stroke="currentColor" stroke-width="1.2" />
        }
        .into_any(),
        IconKind::Placeholder => view! {
            <circle cx="8" cy="5.6" r="2.4" stroke="currentColor" stroke-width="1.2" stroke-dasharray="1.4 1.4" />
            <path d="M3.2 13.2C3.6 10.8 5.4 9.6 8 9.6S12.4 10.8 12.8 13.2" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-dasharray="1.4 1.6" />
        }
        .into_any(),
        IconKind::Calendar => view! {
            <rect x="2.2" y="3.2" width="11.6" height="10.6" rx="1.6" stroke="currentColor" stroke-width="1.2" />
            <line x1="2.2" y1="6.6" x2="13.8" y2="6.6" stroke="currentColor" stroke-width="1.2" />
            <line x1="5.4" y1="1.8" x2="5.4" y2="4.4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
            <line x1="10.6" y1="1.8" x2="10.6" y2="4.4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::Pencil => view! {
            <path d="M2.6 13.4L3.2 10.4L10.6 3L13 5.4L5.6 12.8Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" />
            <line x1="9.2" y1="4.4" x2="11.6" y2="6.8" stroke="currentColor" stroke-width="1.2" />
        }
        .into_any(),
        IconKind::Close => view! {
            <line x1="4" y1="4" x2="12" y2="12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            <line x1="12" y1="4" x2="4" y2="12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::Bin => view! {
            <path d="M3.5 4.5H12.5M6.5 4.5V3H9.5V4.5M4.5 4.5L5.1 13H10.9L11.5 4.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" />
            <line x1="6.8" y1="6.8" x2="7" y2="10.8" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
            <line x1="9.2" y1="6.8" x2="9" y2="10.8" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::Timer => view! {
            <circle cx="8" cy="9" r="4.7" stroke="currentColor" stroke-width="1.2" fill="none" />
            <path d="M8 6.4V9H10M6.6 2.4H9.4M8 2.4V4.3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" fill="none" />
        }
        .into_any(),
        IconKind::Plus => view! {
            <line x1="8" y1="3.5" x2="8" y2="12.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            <line x1="3.5" y1="8" x2="12.5" y2="8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::Tick => view! {
            <polyline points="3.5,8.5 6.8,11.8 12.5,4.8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
        }
        .into_any(),
        IconKind::Play => view! {
            <path d="M5.5 3.5L12 8L5.5 12.5Z" fill="currentColor" />
        }
        .into_any(),
        IconKind::Person => view! {
            <circle cx="8" cy="8" r="6.9" stroke="currentColor" stroke-width="1.2" />
            <circle cx="8" cy="6.3" r="2.2" fill="currentColor" />
            <path d="M3.6 12.6C4.6 10.7 6.1 9.9 8 9.9S11.4 10.7 12.4 12.6" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
        }
        .into_any(),
        IconKind::Alert => view! {
            <path d="M8 2L14.5 13.5H1.5Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" />
            <line x1="8" y1="6.3" x2="8" y2="9.6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
            <circle cx="8" cy="11.6" r="0.8" fill="currentColor" />
        }
        .into_any(),
    };
    view! {
        <svg width=size height=size viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
            {body}
        </svg>
    }
}
// }}}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconKind {
    Back,
    Gantt,
    List,
    Allocation,
    Settings,
    Resources,
    Users,
    Search,
    Today,
    AddTask,
    AddMilestone,
    AddGroup,
    AddBuffer,
    AddDelay,
    Alert,
    Person,
    Tick,
    Calendar,
    Placeholder,
    Circle,
    Pause,
    Drop,
    Pin,
    Reset,
    Data,
    Pencil,
    Close,
    Bin,
    Timer,
    Plus,
    Play,
}
