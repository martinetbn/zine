use bevy::prelude::*;

/// Marker for the crosshair UI element.
#[derive(Component)]
pub struct Crosshair;

pub fn setup_crosshair(mut commands: Commands) {
    // Crosshair container (centered on screen)
    commands
        .spawn((
            Crosshair,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                position_type: PositionType::Absolute,
                ..default()
            },
        ))
        .with_children(|parent| {
            // Crosshair dot
            parent.spawn((
                Node {
                    width: Val::Px(4.0),
                    height: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
            ));
        });
}

pub fn cleanup_crosshair(mut commands: Commands, query: Query<Entity, With<Crosshair>>) {
    for entity in query.iter() {
        commands.entity(entity).despawn_recursive();
    }
}
