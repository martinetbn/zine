use bevy::prelude::*;

use super::components::{NotificationRoot, NotificationText};

/// Event to display a notification message.
#[derive(Event)]
pub struct NotificationEvent(pub String);

/// Marker for the in-game UI camera.
#[derive(Component)]
pub struct InGameUICamera;

/// Duration in seconds for notifications to display.
const NOTIFICATION_DURATION: f32 = 3.0;

/// Sets up the notification container UI.
pub fn setup_notification_ui(mut commands: Commands) {
    // Spawn a UI camera for in-game notifications
    commands.spawn((InGameUICamera, Camera2d, Camera { order: 1, ..default() }));

    commands.spawn((
        NotificationRoot,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(20.0),
            top: Val::Px(20.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            ..default()
        },
    ));
}

/// Cleans up the notification container and UI camera.
pub fn cleanup_notification_ui(
    mut commands: Commands,
    root_query: Query<Entity, With<NotificationRoot>>,
    camera_query: Query<Entity, With<InGameUICamera>>,
) {
    for entity in root_query.iter() {
        commands.entity(entity).despawn_recursive();
    }
    for entity in camera_query.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

/// Spawns notification text when events are received.
pub fn display_notifications(
    mut commands: Commands,
    mut events: EventReader<NotificationEvent>,
    root_query: Query<Entity, With<NotificationRoot>>,
) {
    let Ok(root) = root_query.get_single() else {
        return;
    };

    for event in events.read() {
        commands.entity(root).with_children(|parent| {
            parent.spawn((
                NotificationText(NOTIFICATION_DURATION),
                Text::new(&event.0),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                Node {
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                    ..default()
                },
            ));
        });
    }
}

/// Updates notification timers and removes expired notifications.
pub fn update_notifications(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut NotificationText)>,
) {
    for (entity, mut notification) in query.iter_mut() {
        notification.0 -= time.delta_secs();
        if notification.0 <= 0.0 {
            commands.entity(entity).despawn_recursive();
        }
    }
}
