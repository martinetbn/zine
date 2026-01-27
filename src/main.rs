use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, PresentMode},
};

// Player marker component
#[derive(Component)]
struct Player;

// Camera controller for mouse look
#[derive(Component)]
struct CameraController {
    pitch: f32,
    yaw: f32,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            yaw: std::f32::consts::PI, // Start facing -Z direction
        }
    }
}

// Velocity component for physics
#[derive(Component, Default)]
struct Velocity(Vec3);

// Player physics constants
const PLAYER_SPEED: f32 = 5.0;
const JUMP_VELOCITY: f32 = 8.0;
const GRAVITY: f32 = 20.0;
const PLAYER_HEIGHT: f32 = 1.8;
const GROUND_LEVEL: f32 = 0.0;

// Mouse look constants
const MOUSE_SENSITIVITY: f32 = 0.003;
const PITCH_LIMIT: f32 = 1.5; // ~86 degrees, just under 90

// Room bounds for collision
const ROOM_HALF_WIDTH: f32 = 4.8; // slightly less than 5.0 to account for walls
const ROOM_HALF_DEPTH: f32 = 4.8;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Zine".to_string(),
                    present_mode: PresentMode::AutoNoVsync,
                    ..default()
                }),
                ..default()
            }),
        )
        .add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            LogDiagnosticsPlugin::default(),
        ))
        .add_systems(Startup, (setup, grab_cursor))
        .add_systems(Update, (mouse_look, center_cursor, player_movement, apply_gravity, apply_velocity, toggle_cursor_grab))
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Room dimensions
    let room_width = 10.0;
    let room_depth = 10.0;
    let room_height = 4.0;
    let wall_thickness = 0.2;

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
        Mesh3d(meshes.add(Plane3d::default().mesh().size(room_width, room_depth))),
        MeshMaterial3d(floor_material),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Ceiling
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(room_width, room_depth))),
        MeshMaterial3d(ceiling_material),
        Transform::from_xyz(0.0, room_height, 0.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::PI)),
    ));

    // Back wall (negative Z)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(room_width, room_height, wall_thickness))),
        MeshMaterial3d(wall_material.clone()),
        Transform::from_xyz(0.0, room_height / 2.0, -room_depth / 2.0),
    ));

    // Front wall (positive Z)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(room_width, room_height, wall_thickness))),
        MeshMaterial3d(wall_material.clone()),
        Transform::from_xyz(0.0, room_height / 2.0, room_depth / 2.0),
    ));

    // Left wall (negative X)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(wall_thickness, room_height, room_depth))),
        MeshMaterial3d(wall_material.clone()),
        Transform::from_xyz(-room_width / 2.0, room_height / 2.0, 0.0),
    ));

    // Right wall (positive X)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(wall_thickness, room_height, room_depth))),
        MeshMaterial3d(wall_material),
        Transform::from_xyz(room_width / 2.0, room_height / 2.0, 0.0),
    ));

    // Point light (ceiling light)
    // Note: shadows disabled for better performance on WSL
    commands.spawn((
        PointLight {
            shadows_enabled: false,
            intensity: 2_000_000.0,
            range: 20.0,
            ..default()
        },
        Transform::from_xyz(0.0, room_height - 0.5, 0.0),
    ));

    // Player (Camera)
    commands.spawn((
        Player,
        CameraController::default(),
        Velocity::default(),
        Camera3d::default(),
        Transform::from_xyz(0.0, PLAYER_HEIGHT, 4.0).looking_at(Vec3::new(0.0, PLAYER_HEIGHT, 0.0), Vec3::Y),
    ));
}

fn grab_cursor(mut windows: Query<&mut Window>) {
    let mut window = windows.single_mut();
    window.cursor_options.grab_mode = CursorGrabMode::Locked;
    window.cursor_options.visible = false;
}

fn toggle_cursor_grab(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window>,
) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        let mut window = windows.single_mut();
        match window.cursor_options.grab_mode {
            CursorGrabMode::None => {
                window.cursor_options.grab_mode = CursorGrabMode::Locked;
                window.cursor_options.visible = false;
            }
            _ => {
                window.cursor_options.grab_mode = CursorGrabMode::None;
                window.cursor_options.visible = true;
            }
        }
    }
}

fn mouse_look(
    mut mouse_motion: EventReader<MouseMotion>,
    mut query: Query<(&mut Transform, &mut CameraController), With<Player>>,
    windows: Query<&Window>,
) {
    let window = windows.single();

    // Only process mouse look when cursor is grabbed
    if window.cursor_options.grab_mode == CursorGrabMode::None {
        mouse_motion.clear();
        return;
    }

    let (mut transform, mut controller) = query.single_mut();

    for event in mouse_motion.read() {
        controller.yaw -= event.delta.x * MOUSE_SENSITIVITY;
        controller.pitch -= event.delta.y * MOUSE_SENSITIVITY;

        // Clamp pitch to prevent flipping
        controller.pitch = controller.pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    // Apply rotation
    transform.rotation = Quat::from_euler(EulerRot::YXZ, controller.yaw, controller.pitch, 0.0);
}

fn center_cursor(mut windows: Query<&mut Window>) {
    let mut window = windows.single_mut();

    // Only center cursor when it's grabbed and window is focused
    if window.cursor_options.grab_mode != CursorGrabMode::None && window.focused {
        let center = Vec2::new(window.width() / 2.0, window.height() / 2.0);
        window.set_cursor_position(Some(center));
    }
}

fn player_movement(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&Transform, &mut Velocity), With<Player>>,
) {
    let (transform, mut velocity) = query.single_mut();

    // Get movement direction from WASD
    let mut direction = Vec3::ZERO;

    if keyboard_input.pressed(KeyCode::KeyW) {
        direction.z -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyS) {
        direction.z += 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyA) {
        direction.x -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyD) {
        direction.x += 1.0;
    }

    // Normalize diagonal movement
    if direction.length() > 0.0 {
        direction = direction.normalize();
    }

    // Apply movement relative to camera facing direction (only yaw)
    let forward = transform.forward();
    let forward_flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_flat = Vec3::new(-forward.z, 0.0, forward.x).normalize_or_zero();

    let move_direction = forward_flat * -direction.z + right_flat * direction.x;

    // Set horizontal velocity
    velocity.0.x = move_direction.x * PLAYER_SPEED;
    velocity.0.z = move_direction.z * PLAYER_SPEED;

    // Jump (only when grounded)
    let is_grounded = transform.translation.y <= GROUND_LEVEL + PLAYER_HEIGHT + 0.01;
    if keyboard_input.just_pressed(KeyCode::Space) && is_grounded {
        velocity.0.y = JUMP_VELOCITY;
    }
}

fn apply_gravity(
    time: Res<Time>,
    mut query: Query<(&Transform, &mut Velocity), With<Player>>,
) {
    let (transform, mut velocity) = query.single_mut();

    let is_grounded = transform.translation.y <= GROUND_LEVEL + PLAYER_HEIGHT + 0.01;

    if !is_grounded {
        velocity.0.y -= GRAVITY * time.delta_secs();
    }
}

fn apply_velocity(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut Velocity), With<Player>>,
) {
    let (mut transform, mut velocity) = query.single_mut();

    // Apply velocity to position
    transform.translation += velocity.0 * time.delta_secs();

    // Ground collision
    if transform.translation.y < GROUND_LEVEL + PLAYER_HEIGHT {
        transform.translation.y = GROUND_LEVEL + PLAYER_HEIGHT;
        velocity.0.y = 0.0;
    }

    // Wall collisions (keep player inside room)
    transform.translation.x = transform.translation.x.clamp(-ROOM_HALF_WIDTH, ROOM_HALF_WIDTH);
    transform.translation.z = transform.translation.z.clamp(-ROOM_HALF_DEPTH, ROOM_HALF_DEPTH);
}
