use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::components::{Interactable, ScreenControlButton};
use crate::network::server::GameServer;

/// Resource tracking what the player is currently looking at.
#[derive(Resource, Default)]
pub struct LookingAt {
    pub entity: Option<Entity>,
    pub distance: f32,
}

/// Event fired when the screen control button is activated.
#[derive(Event)]
pub struct ScreenControlEvent;

/// Maximum interaction distance.
const INTERACTION_DISTANCE: f32 = 4.0;

/// System to raycast from camera and detect what player is looking at.
pub fn update_looking_at(
    mut looking_at: ResMut<LookingAt>,
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    interactables: Query<(Entity, &GlobalTransform), With<Interactable>>,
) {
    let Ok(camera_transform) = camera_query.get_single() else {
        return;
    };

    let ray_origin = camera_transform.translation();
    let ray_direction = camera_transform.forward().as_vec3();

    // Simple distance-based check for now (proper raycasting would use bevy_rapier or similar)
    // We'll check against interactable positions with a tolerance
    let mut closest: Option<(Entity, f32)> = None;

    for (entity, transform) in interactables.iter() {
        let to_object = transform.translation() - ray_origin;
        let distance_along_ray = to_object.dot(ray_direction);

        if distance_along_ray < 0.0 || distance_along_ray > INTERACTION_DISTANCE {
            continue;
        }

        // Point on ray closest to the object center
        let closest_point = ray_origin + ray_direction * distance_along_ray;
        let distance_to_center = (transform.translation() - closest_point).length();

        // Use a simple sphere approximation for hit detection
        let hit_radius = 0.4; // Generous hit box
        if distance_to_center < hit_radius {
            if closest.is_none() || distance_along_ray < closest.unwrap().1 {
                closest = Some((entity, distance_along_ray));
            }
        }
    }

    looking_at.entity = closest.map(|(e, _)| e);
    looking_at.distance = closest.map(|(_, d)| d).unwrap_or(0.0);
}

/// System to highlight interactables when looking at them.
pub fn highlight_interactables(
    looking_at: Res<LookingAt>,
    interactables: Query<(Entity, &Interactable, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, interactable, material_handle) in interactables.iter() {
        if let Some(material) = materials.get_mut(&material_handle.0) {
            if Some(entity) == looking_at.entity {
                material.base_color = interactable.hover_color;
            } else {
                material.base_color = interactable.normal_color;
            }
        }
    }
}

/// System to handle right-click interaction on the screen control button (host only).
pub fn handle_screen_control_interaction(
    mouse_input: Res<ButtonInput<MouseButton>>,
    looking_at: Res<LookingAt>,
    button_query: Query<Entity, With<ScreenControlButton>>,
    server: Option<Res<GameServer>>,
    mut event_writer: EventWriter<ScreenControlEvent>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    // Only host can configure the screen
    if server.is_none() {
        return;
    }

    // Only when cursor is grabbed (in game)
    let Ok(window) = windows.get_single() else {
        return;
    };
    if window.cursor_options.visible {
        return;
    }

    if mouse_input.just_pressed(MouseButton::Right) {
        if let Some(looking_entity) = looking_at.entity {
            if button_query.get(looking_entity).is_ok() {
                info!("Screen control button activated!");
                event_writer.send(ScreenControlEvent);
            }
        }
    }
}

/// Temporary system to respond to screen control events (placeholder for future config UI).
pub fn on_screen_control_event(mut events: EventReader<ScreenControlEvent>) {
    for _ in events.read() {
        info!("Screen configuration will open here in the future!");
    }
}
