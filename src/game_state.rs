use bevy::prelude::*;

/// Main application states controlling game flow.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    Hosting,
    Browsing,
    Connecting,
    InGame,
}
