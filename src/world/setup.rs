use bevy::prelude::*;

use crate::player::{CameraController, Player, Velocity, PLAYER_HEIGHT};

use super::components::{Interactable, Screen, ScreenControlButton, ScreenFrame, WorldEntity};
use super::{ROOM_DEPTH, ROOM_HEIGHT, ROOM_WIDTH, WALL_THICKNESS};

// Screen dimensions (base dimensions, can be scaled by aspect ratio)
pub const SCREEN_WIDTH: f32 = 6.0;
pub const SCREEN_HEIGHT: f32 = 3.0;
pub const SCREEN_Y: f32 = 2.2; // Center height of the screen
pub const SCREEN_DEPTH: f32 = 0.05;
pub const FRAME_THICKNESS: f32 = 0.08;

// Control button dimensions
pub const BUTTON_SIZE: f32 = 0.3;
pub const BUTTON_OFFSET_X: f32 = 0.3; // Distance from screen edge

pub fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Materials
    let floor_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.4, 0.35, 0.3),
        ..default()
    });
    let wall_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.75, 0.7),
        ..default()
    });
    let ceiling_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.9, 0.9),
        ..default()
    });

    // Floor
    commands.spawn((
        WorldEntity,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(ROOM_WIDTH, ROOM_DEPTH))),
        MeshMaterial3d(floor_material),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Ceiling
    commands.spawn((
        WorldEntity,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(ROOM_WIDTH, ROOM_DEPTH))),
        MeshMaterial3d(ceiling_material),
        Transform::from_xyz(0.0, ROOM_HEIGHT, 0.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::PI)),
    ));

    // Back wall (negative Z)
    commands.spawn((
        WorldEntity,
        Mesh3d(meshes.add(Cuboid::new(ROOM_WIDTH, ROOM_HEIGHT, WALL_THICKNESS))),
        MeshMaterial3d(wall_material.clone()),
        Transform::from_xyz(0.0, ROOM_HEIGHT / 2.0, -ROOM_DEPTH / 2.0),
    ));

    // Front wall (positive Z)
    commands.spawn((
        WorldEntity,
        Mesh3d(meshes.add(Cuboid::new(ROOM_WIDTH, ROOM_HEIGHT, WALL_THICKNESS))),
        MeshMaterial3d(wall_material.clone()),
        Transform::from_xyz(0.0, ROOM_HEIGHT / 2.0, ROOM_DEPTH / 2.0),
    ));

    // Left wall (negative X)
    commands.spawn((
        WorldEntity,
        Mesh3d(meshes.add(Cuboid::new(WALL_THICKNESS, ROOM_HEIGHT, ROOM_DEPTH))),
        MeshMaterial3d(wall_material.clone()),
        Transform::from_xyz(-ROOM_WIDTH / 2.0, ROOM_HEIGHT / 2.0, 0.0),
    ));

    // Right wall (positive X)
    commands.spawn((
        WorldEntity,
        Mesh3d(meshes.add(Cuboid::new(WALL_THICKNESS, ROOM_HEIGHT, ROOM_DEPTH))),
        MeshMaterial3d(wall_material),
        Transform::from_xyz(ROOM_WIDTH / 2.0, ROOM_HEIGHT / 2.0, 0.0),
    ));

    // Point light (ceiling light)
    commands.spawn((
        WorldEntity,
        PointLight {
            shadows_enabled: false,
            intensity: 2_000_000.0,
            range: 20.0,
            ..default()
        },
        Transform::from_xyz(0.0, ROOM_HEIGHT - 0.5, 0.0),
    ));

    // Screen on back wall
    let screen_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.05, 0.05, 0.08),
        emissive: Color::linear_rgb(0.02, 0.02, 0.03).into(),
        ..default()
    });

    commands.spawn((
        WorldEntity,
        Screen,
        Mesh3d(meshes.add(Cuboid::new(SCREEN_WIDTH, SCREEN_HEIGHT, SCREEN_DEPTH))),
        MeshMaterial3d(screen_material),
        Transform::from_xyz(0.0, SCREEN_Y, -ROOM_DEPTH / 2.0 + WALL_THICKNESS / 2.0 + 0.03),
    ));

    // Screen frame/border
    let frame_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.15, 0.15, 0.15),
        ..default()
    });

    let frame_z = -ROOM_DEPTH / 2.0 + WALL_THICKNESS / 2.0 + 0.02;

    // Top frame
    commands.spawn((
        WorldEntity,
        ScreenFrame::Top,
        Mesh3d(meshes.add(Cuboid::new(
            SCREEN_WIDTH + FRAME_THICKNESS * 2.0,
            FRAME_THICKNESS,
            0.06,
        ))),
        MeshMaterial3d(frame_material.clone()),
        Transform::from_xyz(
            0.0,
            SCREEN_Y + SCREEN_HEIGHT / 2.0 + FRAME_THICKNESS / 2.0,
            frame_z,
        ),
    ));
    // Bottom frame
    commands.spawn((
        WorldEntity,
        ScreenFrame::Bottom,
        Mesh3d(meshes.add(Cuboid::new(
            SCREEN_WIDTH + FRAME_THICKNESS * 2.0,
            FRAME_THICKNESS,
            0.06,
        ))),
        MeshMaterial3d(frame_material.clone()),
        Transform::from_xyz(
            0.0,
            SCREEN_Y - SCREEN_HEIGHT / 2.0 - FRAME_THICKNESS / 2.0,
            frame_z,
        ),
    ));
    // Left frame
    commands.spawn((
        WorldEntity,
        ScreenFrame::Left,
        Mesh3d(meshes.add(Cuboid::new(FRAME_THICKNESS, SCREEN_HEIGHT, 0.06))),
        MeshMaterial3d(frame_material.clone()),
        Transform::from_xyz(
            -SCREEN_WIDTH / 2.0 - FRAME_THICKNESS / 2.0,
            SCREEN_Y,
            frame_z,
        ),
    ));
    // Right frame
    commands.spawn((
        WorldEntity,
        ScreenFrame::Right,
        Mesh3d(meshes.add(Cuboid::new(FRAME_THICKNESS, SCREEN_HEIGHT, 0.06))),
        MeshMaterial3d(frame_material),
        Transform::from_xyz(SCREEN_WIDTH / 2.0 + FRAME_THICKNESS / 2.0, SCREEN_Y, frame_z),
    ));

    // Screen control button (right side of screen)
    let button_normal_color = Color::srgb(0.3, 0.5, 0.3);
    let button_material = materials.add(StandardMaterial {
        base_color: button_normal_color,
        ..default()
    });

    commands.spawn((
        WorldEntity,
        ScreenControlButton,
        Interactable {
            normal_color: button_normal_color,
            hover_color: Color::srgb(0.4, 0.7, 0.4),
        },
        Mesh3d(meshes.add(Cuboid::new(BUTTON_SIZE, BUTTON_SIZE, 0.05))),
        MeshMaterial3d(button_material),
        Transform::from_xyz(
            SCREEN_WIDTH / 2.0 + FRAME_THICKNESS + BUTTON_OFFSET_X + BUTTON_SIZE / 2.0,
            SCREEN_Y - SCREEN_HEIGHT / 2.0 + BUTTON_SIZE / 2.0,
            -ROOM_DEPTH / 2.0 + WALL_THICKNESS / 2.0 + 0.03,
        ),
    ));

    // Player (Camera)
    commands.spawn((
        WorldEntity,
        Player,
        CameraController::default(),
        Velocity::default(),
        Camera3d::default(),
        Transform::from_xyz(0.0, PLAYER_HEIGHT, 4.0)
            .looking_at(Vec3::new(0.0, PLAYER_HEIGHT, 0.0), Vec3::Y),
    ));
}

/// Cleans up all world entities.
pub fn cleanup_world(mut commands: Commands, query: Query<Entity, With<WorldEntity>>) {
    for entity in query.iter() {
        commands.entity(entity).despawn_recursive();
    }
}
