use bevy::prelude::*;

/// Marker for the cinema screen.
#[derive(Component)]
pub struct Screen;

/// Marker for the screen control button.
#[derive(Component)]
pub struct ScreenControlButton;

/// Component for interactable objects that can be right-clicked.
#[derive(Component)]
pub struct Interactable {
    pub hover_color: Color,
    pub normal_color: Color,
}
