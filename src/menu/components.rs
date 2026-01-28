use bevy::prelude::*;

/// Marker for the menu camera.
#[derive(Component)]
pub struct MenuCamera;

/// Marker for the main menu root UI node.
#[derive(Component)]
pub struct MainMenuRoot;

/// Marker for the host game button.
#[derive(Component)]
pub struct HostButton;

/// Marker for the join game button.
#[derive(Component)]
pub struct JoinButton;

/// Marker for the back button.
#[derive(Component)]
pub struct BackButton;

/// Marker for the browser UI root.
#[derive(Component)]
pub struct BrowserRoot;

/// Marker for the session list container.
#[derive(Component)]
pub struct SessionList;

/// Marker for a session entry button, contains the session index.
#[derive(Component)]
pub struct SessionEntry(pub usize);

/// Marker for the "Searching..." text.
#[derive(Component)]
pub struct SearchingText;

/// Marker for the notification container in the top-left.
#[derive(Component)]
pub struct NotificationRoot;

/// Marker for a notification text element with remaining display time.
#[derive(Component)]
pub struct NotificationText(pub f32);
