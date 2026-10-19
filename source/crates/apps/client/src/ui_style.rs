//! The look the panels and the inventory bar share: their colors, their font, the write of a line
//! of text, and the one widget of a panel and its rows.

use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::system::{Commands, Query};
use bevy::text::{TextColor, TextFont};
use bevy::ui::widget::Text;
use bevy::ui::{BackgroundColor, FlexDirection, Node, PositionType, UiRect, Val};

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

/// A panel of the window, hidden as it spawns: its root, its line of the player's amounts, and
/// the holder of its rows.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Panel {
    pub(crate) root: Entity,
    pub(crate) funds: Entity,
    pub(crate) rows: Entity,
}

/// The corner of the window a panel stands in, 12 px in from its edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Corner {
    TopRight,
    BottomRight,
}

/// A row of a panel: the row, and its lines of name and of value.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PanelRow {
    pub(crate) row: Entity,
    pub(crate) name: Entity,
    pub(crate) value: Entity,
}

/// How far a panel stands in from the window's edges, in pixels.
const PANEL_INSET: f32 = 12.0;

impl Panel {
    /// A panel `width` pixels wide in the window's `corner`.
    pub(crate) fn spawn(commands: &mut Commands<'_, '_>, corner: Corner, width: f32) -> Self {
        let inset = Val::Px(PANEL_INSET);
        let (top, bottom) = match corner {
            Corner::TopRight => (inset, Val::Auto),
            Corner::BottomRight => (Val::Auto, inset),
        };
        let root = Node {
            position_type: PositionType::Absolute,
            right: inset,
            top,
            bottom,
            width: Val::Px(width),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(8.0)),
            row_gap: Val::Px(4.0),
            ..Node::default()
        };
        let root = commands
            .spawn((root, BackgroundColor(BACK), Visibility::Hidden))
            .id();
        let funds = commands
            .spawn((Text::default(), font(), TextColor(TEXT), ChildOf(root)))
            .id();
        let rows = Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            ..Node::default()
        };
        let rows = commands.spawn((rows, ChildOf(root))).id();
        Self { root, funds, rows }
    }
}

impl PanelRow {
    /// A row of the panel's `rows`, of a line of `name` and an empty line of value.
    pub(crate) fn spawn(commands: &mut Commands<'_, '_>, rows: Entity, name: &str) -> Self {
        let line = Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(12.0),
            ..Node::default()
        };
        let row = commands.spawn((line, ChildOf(rows))).id();
        let mut cell = |text: Text| {
            commands
                .spawn((text, font(), TextColor(TEXT), ChildOf(row)))
                .id()
        };
        let name = cell(Text::new(name));
        let value = cell(Text::default());
        Self { row, name, value }
    }
}
