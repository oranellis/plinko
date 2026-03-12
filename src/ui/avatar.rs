//! Avatar image helpers: resizing on upload and per-window display cache.

use std::collections::HashMap;

use skia_safe::{
    AlphaType, ColorType, Data, EncodedImageFormat, IRect, Image, ImageInfo, Paint, PaintStyle,
    Rect, surfaces,
};

use crate::data::UserId;

/// Maximum edge length (in pixels) of the stored avatar.
/// Images are cropped to a square and scaled to this before being stored.
const AVATAR_MAX_PX: i32 = 256;

/// Decode `bytes`, centre-crop to a square, scale to [`AVATAR_MAX_PX`] × [`AVATAR_MAX_PX`],
/// and re-encode as PNG.  Returns the processed PNG bytes, or `None` if the
/// input cannot be decoded or the re-encoding fails.
pub fn resize_avatar(bytes: &[u8]) -> Option<Vec<u8>> {
    let data = Data::new_copy(bytes);
    let src = Image::from_encoded(data)?;

    let src_w = src.width() as f32;
    let src_h = src.height() as f32;

    // Centre-crop to a square
    let crop_size = src_w.min(src_h);
    let src_x = ((src_w - crop_size) / 2.0) as i32;
    let src_y = ((src_h - crop_size) / 2.0) as i32;
    let crop_size_i = crop_size as i32;

    let info = ImageInfo::new(
        (AVATAR_MAX_PX, AVATAR_MAX_PX),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );

    let mut surface = surfaces::raster(&info, None, None)?;
    let canvas = surface.canvas();

    let dst = Rect::from_iwh(AVATAR_MAX_PX, AVATAR_MAX_PX);
    let src_rect = IRect::from_xywh(src_x, src_y, crop_size_i, crop_size_i);
    let src_rect_f = Rect::from(src_rect);

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Fill);

    canvas.draw_image_rect(
        &src,
        Some((&src_rect_f, skia_safe::canvas::SrcRectConstraint::Fast)),
        dst,
        &paint,
    );

    let image = surface.image_snapshot();
    let encoded = image.encode(None, EncodedImageFormat::PNG, None)?;
    Some(encoded.as_bytes().to_vec())
}

/// A lightweight cache that keeps decoded Skia [`Image`]s keyed by [`UserId`].
///
/// Call [`AvatarCache::get`] each frame — the image is decoded once and reused
/// until [`AvatarCache::invalidate`] is called (e.g. after an edit).
#[derive(Default)]
pub struct AvatarCache {
    map: HashMap<UserId, Option<Image>>,
}

impl AvatarCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a cached [`Image`] for `user_id`, decoding `avatar_bytes` on first access.
    /// Returns `None` if there are no bytes or if decoding fails.
    pub fn get(&mut self, user_id: UserId, avatar_bytes: Option<&Vec<u8>>) -> Option<&Image> {
        match avatar_bytes {
            None => None,
            Some(bytes) => self
                .map
                .entry(user_id)
                .or_insert_with(|| {
                    let d = Data::new_copy(bytes);
                    Image::from_encoded(d)
                })
                .as_ref(),
        }
    }

    /// Drop the cached entry for `user_id` so it will be re-decoded next access.
    pub fn invalidate(&mut self, user_id: UserId) {
        self.map.remove(&user_id);
    }

    /// Clear the entire cache.
    pub fn clear(&mut self) {
        self.map.clear();
    }
}
