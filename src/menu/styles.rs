use bevy::prelude::*;

pub const NORMAL_BUTTON: Color = Color::srgb(0.15, 0.15, 0.15);
pub const HOVERED_BUTTON: Color = Color::srgb(0.25, 0.25, 0.25);
pub const PRESSED_BUTTON: Color = Color::srgb(0.35, 0.65, 0.35);

pub const BUTTON_TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 0.9);
pub const TITLE_TEXT_COLOR: Color = Color::srgb(1.0, 1.0, 1.0);

pub fn button_style() -> Node {
    Node {
        width: Val::Px(250.0),
        height: Val::Px(65.0),
        margin: UiRect::all(Val::Px(10.0)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

pub fn button_text_style() -> TextFont {
    TextFont {
        font_size: 28.0,
        ..default()
    }
}

pub fn title_text_style() -> TextFont {
    TextFont {
        font_size: 64.0,
        ..default()
    }
}
