//! The look the shop panel and the inventory bar share: their colors, their font, and the write
//! of a line of text.

use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::system::Query;
use bevy::text::TextFont;
use bevy::ui::widget::Text;

/// A line's color, bright, and dimmed, as an item the avatar cannot buy now is.
pub(crate) const TEXT: Color = Color::srgb(0.95, 0.92, 0.85);
pub(crate) const DIMMED: Color = Color::srgb(0.45, 0.45, 0.45);
/// A panel's background.
pub(crate) const BACK: Color = Color::srgba(0.05, 0.05, 0.08, 0.85);
const FONT_SIZE: f32 = 16.0;

/// The font of every line.
pub(crate) fn font() -> TextFont {
    TextFont::from_font_size(FONT_SIZE)
}

/// Sets the text of `entity` to `line`, only when it differs, so an unchanged line lays out
/// nothing again.
pub(crate) fn set_text(texts: &mut Query<'_, '_, &mut Text>, entity: Entity, line: &str) {
    if let Ok(mut text) = texts.get_mut(entity)
        && text.0 != line
    {
        line.clone_into(&mut text.0);
    }
}
