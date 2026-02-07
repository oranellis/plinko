use skia_safe::{Font, FontMgr, FontStyle, Path, TextBlob};

use super::layout::{BUTTON_COUNT, ICON_SIZE};
use super::toolbar::{
    build_icon_daily, build_icon_planning, build_icon_settings, build_icon_undo, build_icon_redo,
};

pub struct RenderCache {
    #[allow(dead_code)]
    pub font: Font,
    pub icon_paths: [Path; BUTTON_COUNT],
    pub daily_label: TextBlob,
    pub left_panel_label: TextBlob,
    pub right_panel_label: TextBlob,
    pub settings_label: TextBlob,
}

impl RenderCache {
    pub fn new() -> Self {
        let font_mgr = FontMgr::new();
        let typeface = font_mgr
            .match_family_style("sans-serif", FontStyle::normal())
            .or_else(|| font_mgr.legacy_make_typeface(None, FontStyle::normal()));
        let font = match typeface {
            Some(tf) => Font::from_typeface(tf, 16.0),
            None => Font::default(),
        };

        let icon_paths = [
            build_icon_daily(ICON_SIZE, ICON_SIZE),
            build_icon_planning(ICON_SIZE, ICON_SIZE),
            build_icon_settings(ICON_SIZE, ICON_SIZE),
            build_icon_undo(ICON_SIZE, ICON_SIZE),
            build_icon_redo(ICON_SIZE, ICON_SIZE),
        ];

        let daily_label = TextBlob::new("Daily", &font).expect("text blob");
        let left_panel_label = TextBlob::new("Left Panel", &font).expect("text blob");
        let right_panel_label = TextBlob::new("Right Panel", &font).expect("text blob");
        let settings_label = TextBlob::new("Settings", &font).expect("text blob");

        Self {
            font,
            icon_paths,
            daily_label,
            left_panel_label,
            right_panel_label,
            settings_label,
        }
    }
}
