//! Pre-built Skia render resources that are expensive to construct every frame.

use skia_safe::{Font, FontMgr, FontStyle, Path, TextBlob};

use super::icons::{build_icon_daily, build_icon_planning, build_icon_settings};
use super::layout::HOME_CARD_ICON_SIZE;

/// Holds Skia paths and text blobs that are built once at startup and reused
/// every frame.  Passed as a shared reference to every page renderer.
pub struct RenderCache {
    #[allow(dead_code)]
    pub font: Font,
    pub home_icon_paths: [Path; 3],
    pub home_card_labels: [TextBlob; 3],
    pub daily_label: TextBlob,
    pub left_panel_label: TextBlob,
    pub right_panel_label: TextBlob,
    pub settings_label: TextBlob,
}

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
        let card_font = match typeface {
            Some(tf) => Font::from_typeface(tf, 14.0),
            None => Font::default(),
        };

        let home_icon_paths = [
            build_icon_daily(HOME_CARD_ICON_SIZE, HOME_CARD_ICON_SIZE),
            build_icon_planning(HOME_CARD_ICON_SIZE, HOME_CARD_ICON_SIZE),
            build_icon_settings(HOME_CARD_ICON_SIZE, HOME_CARD_ICON_SIZE),
        ];

        let home_card_labels = [
            TextBlob::new("Daily", &card_font).expect("text blob"),
            TextBlob::new("Planning", &card_font).expect("text blob"),
            TextBlob::new("Settings", &card_font).expect("text blob"),
        ];

        let daily_label = TextBlob::new("Daily", &font).expect("text blob");
        let left_panel_label = TextBlob::new("Left Panel", &font).expect("text blob");
        let right_panel_label = TextBlob::new("Right Panel", &font).expect("text blob");
        let settings_label = TextBlob::new("Settings", &font).expect("text blob");

        Self {
            font,
            home_icon_paths,
            home_card_labels,
            daily_label,
            left_panel_label,
            right_panel_label,
            settings_label,
        }
    }
}
