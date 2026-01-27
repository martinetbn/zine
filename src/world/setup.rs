use bevy::prelude::*;

use crate::player::{CameraController, Player, Velocity, PLAYER_HEIGHT};

use super::components::{Interactable, Screen, ScreenControlButton};
use super::{ROOM_DEPTH, ROOM_HEIGHT, ROOM_WIDTH, WALL_THICKNESS};

// Screen dimensions
const SCREEN_WIDTH: f32 = 6.0;
const SCREEN_HEIGHT: f32 = 3.0;
const SCREEN_Y: f32 = 2.2; // Center height of the screen

// Control button dimensions
const BUTTON_SIZE: f32 = 0.3;
const BUTTON_OFFSET_X: f32 = 0.3; // Distance from screen edge

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

    // Screen on back wall
    let screen_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.05, 0.05, 0.08),
        emissive: Color::linear_rgb(0.02, 0.02, 0.03).into(),
        ..default()
    });

    commands.spawn((
        Screen,
        Mesh3d(meshes.add(Cuboid::new(SCREEN_WIDTH, SCREEN_HEIGHT, 0.05))),
        MeshMaterial3d(screen_material),
        Transform::from_xyz(0.0, SCREEN_Y, -ROOM_DEPTH / 2.0 + WALL_THICKNESS / 2.0 + 0.03),
    ));

    // Screen frame/border
    let frame_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.15, 0.15, 0.15),
        ..default()
    });

    let frame_thickness = 0.08;
    // Top frame
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(SCREEN_WIDTH + frame_thickness * 2.0, frame_thickness, 0.06))),
        MeshMaterial3d(frame_material.clone()),
        Transform::from_xyz(
            0.0,
            SCREEN_Y + SCREEN_HEIGHT / 2.0 + frame_thickness / 2.0,
            -ROOM_DEPTH / 2.0 + WALL_THICKNESS / 2.0 + 0.02,
        ),
    ));
    // Bottom frame
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(SCREEN_WIDTH + frame_thickness * 2.0, frame_thickness, 0.06))),
        MeshMaterial3d(frame_material.clone()),
        Transform::from_xyz(
            0.0,
            SCREEN_Y - SCREEN_HEIGHT / 2.0 - frame_thickness / 2.0,
            -ROOM_DEPTH / 2.0 + WALL_THICKNESS / 2.0 + 0.02,
        ),
    ));
    // Left frame
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(frame_thickness, SCREEN_HEIGHT, 0.06))),
        MeshMaterial3d(frame_material.clone()),
        Transform::from_xyz(
            -SCREEN_WIDTH / 2.0 - frame_thickness / 2.0,
            SCREEN_Y,
            -ROOM_DEPTH / 2.0 + WALL_THICKNESS / 2.0 + 0.02,
        ),
    ));
    // Right frame
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(frame_thickness, SCREEN_HEIGHT, 0.06))),
        MeshMaterial3d(frame_material),
        Transform::from_xyz(
            SCREEN_WIDTH / 2.0 + frame_thickness / 2.0,
            SCREEN_Y,
            -ROOM_DEPTH / 2.0 + WALL_THICKNESS / 2.0 + 0.02,
        ),
    ));

    // Screen control button (right side of screen)
    let button_normal_color = Color::srgb(0.3, 0.5, 0.3);
    let button_material = materials.add(StandardMaterial {
        base_color: button_normal_color,
        ..default()
    });

    commands.spawn((
        ScreenControlButton,
        Interactable {
            normal_color: button_normal_color,
            hover_color: Color::srgb(0.4, 0.7, 0.4),
        },
        Mesh3d(meshes.add(Cuboid::new(BUTTON_SIZE, BUTTON_SIZE, 0.05))),
        MeshMaterial3d(button_material),
        Transform::from_xyz(
            SCREEN_WIDTH / 2.0 + frame_thickness + BUTTON_OFFSET_X + BUTTON_SIZE / 2.0,
            SCREEN_Y - SCREEN_HEIGHT / 2.0 + BUTTON_SIZE / 2.0,
            -ROOM_DEPTH / 2.0 + WALL_THICKNESS / 2.0 + 0.03,
        ),
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
