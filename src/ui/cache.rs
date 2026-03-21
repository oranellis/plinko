//! Pre-built Skia render resources that are expensive to construct every frame.

use skia_safe::{Font, FontMgr, FontStyle, Path, TextBlob};

use super::icons::{
    build_icon_allocation, build_icon_calendar_edit, build_icon_daily, build_icon_diamond,
    build_icon_person, build_icon_planning, build_icon_plus, build_icon_settings, build_icon_tag,
    build_icon_today,
};
use super::layout::HOME_CARD_ICON_SIZE;

/// Holds Skia paths and text blobs that are built once at startup and reused
/// every frame.  Passed as a shared reference to every page renderer.
pub struct RenderCache {
    pub font: Font,
    /// Smaller font (12 px) for labels and secondary text.
    pub small_font: Font,
    pub home_icon_paths: [Path; 5],
    pub home_card_labels: [TextBlob; 5],
    /// Person silhouette icon used by the overview toolbar.
    pub icon_person: Path,
    /// Plus / add-task icon used by the overview toolbar.
    pub icon_plus: Path,
    /// Diamond / milestone icon used by the overview toolbar.
    pub icon_diamond: Path,
    /// Hashtag / tag icon used by the tags window button.
    pub icon_tag: Path,
    /// Settings (cogwheel/sliders) icon.
    pub icon_settings: Path,
    /// "Go to today" icon.
    pub icon_today: Path,
    pub daily_label: TextBlob,
    pub left_panel_label: TextBlob,
    pub right_panel_label: TextBlob,
    pub settings_label: TextBlob,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl RenderCache {
    /// Builds all cached resources.  Resolves a sans-serif typeface via
    /// [`FontMgr`] and falls back to [`Font::default()`] if none is found.
    pub fn new() -> Self {
        let font_mgr = FontMgr::new();
        let typeface = font_mgr
            .match_family_style("sans-serif", FontStyle::normal())
            .or_else(|| font_mgr.legacy_make_typeface(None, FontStyle::normal()));
        let font = match &typeface {
            Some(tf) => Font::from_typeface(tf.clone(), 16.0),
            None => Font::default(),
        };
        let small_font = match &typeface {
            Some(tf) => Font::from_typeface(tf.clone(), 12.0),
            None => Font::default(),
        };
        let card_font = match typeface {
            Some(tf) => Font::from_typeface(tf, 14.0),
            None => Font::default(),
        };

        let home_icon_paths = [
            build_icon_daily(HOME_CARD_ICON_SIZE, HOME_CARD_ICON_SIZE),
            build_icon_planning(HOME_CARD_ICON_SIZE, HOME_CARD_ICON_SIZE),
            build_icon_settings(HOME_CARD_ICON_SIZE, HOME_CARD_ICON_SIZE),
            build_icon_allocation(HOME_CARD_ICON_SIZE, HOME_CARD_ICON_SIZE),
            build_icon_calendar_edit(HOME_CARD_ICON_SIZE, HOME_CARD_ICON_SIZE),
        ];
        let icon_person = build_icon_person(32.0, 32.0);
        let icon_plus = build_icon_plus(32.0, 32.0);
        let icon_diamond = build_icon_diamond(32.0, 32.0);
        let icon_tag = build_icon_tag(32.0, 32.0);
        let icon_settings = build_icon_settings(32.0, 32.0);
        let icon_today = build_icon_today(32.0, 32.0);

        let home_card_labels = [
            TextBlob::new("Daily", &card_font).expect("text blob"),
            TextBlob::new("Overview", &card_font).expect("text blob"),
            TextBlob::new("Settings", &card_font).expect("text blob"),
            TextBlob::new("Allocation", &card_font).expect("text blob"),
            TextBlob::new("Calendar", &card_font).expect("text blob"),
        ];

        let daily_label = TextBlob::new("Daily", &font).expect("text blob");
        let left_panel_label = TextBlob::new("Left Panel", &font).expect("text blob");
        let right_panel_label = TextBlob::new("Right Panel", &font).expect("text blob");
        let settings_label = TextBlob::new("Settings", &font).expect("text blob");

        Self {
            font,
            small_font,
            home_icon_paths,
            home_card_labels,
            icon_person,
            icon_plus,
            icon_diamond,
            icon_tag,
            icon_settings,
            icon_today,
            daily_label,
            left_panel_label,
            right_panel_label,
            settings_label,
        }
    }
}
// }}}
