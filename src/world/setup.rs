use bevy::prelude::*;

use crate::player::{CameraController, Player, Velocity, PLAYER_HEIGHT};

use super::{ROOM_DEPTH, ROOM_HEIGHT, ROOM_WIDTH, WALL_THICKNESS};

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
        Mesh3d(meshes.add(Plane3d::default().mesh().size(ROOM_WIDTH, ROOM_DEPTH))),
        MeshMaterial3d(floor_material),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Ceiling
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(ROOM_WIDTH, ROOM_DEPTH))),
        MeshMaterial3d(ceiling_material),
        Transform::from_xyz(0.0, ROOM_HEIGHT, 0.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::PI)),
    ));

    // Back wall (negative Z)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(ROOM_WIDTH, ROOM_HEIGHT, WALL_THICKNESS))),
        MeshMaterial3d(wall_material.clone()),
        Transform::from_xyz(0.0, ROOM_HEIGHT / 2.0, -ROOM_DEPTH / 2.0),
    ));

    // Front wall (positive Z)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(ROOM_WIDTH, ROOM_HEIGHT, WALL_THICKNESS))),
        MeshMaterial3d(wall_material.clone()),
        Transform::from_xyz(0.0, ROOM_HEIGHT / 2.0, ROOM_DEPTH / 2.0),
    ));

    // Left wall (negative X)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(WALL_THICKNESS, ROOM_HEIGHT, ROOM_DEPTH))),
        MeshMaterial3d(wall_material.clone()),
        Transform::from_xyz(-ROOM_WIDTH / 2.0, ROOM_HEIGHT / 2.0, 0.0),
    ));

    // Right wall (positive X)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(WALL_THICKNESS, ROOM_HEIGHT, ROOM_DEPTH))),
        MeshMaterial3d(wall_material),
        Transform::from_xyz(ROOM_WIDTH / 2.0, ROOM_HEIGHT / 2.0, 0.0),
    ));

    // Point light (ceiling light)
    commands.spawn((
        PointLight {
            shadows_enabled: false,
            intensity: 2_000_000.0,
            range: 20.0,
            ..default()
        },
        Transform::from_xyz(0.0, ROOM_HEIGHT - 0.5, 0.0),
    ));

    // Player (Camera)
    commands.spawn((
        Player,
        CameraController::default(),
        Velocity::default(),
        Camera3d::default(),
        Transform::from_xyz(0.0, PLAYER_HEIGHT, 4.0)
            .looking_at(Vec3::new(0.0, PLAYER_HEIGHT, 0.0), Vec3::Y),
    ));
}
