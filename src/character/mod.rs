use bevy::{animation::prelude::AnimationTransitions, app::Animation, gltf::Gltf, prelude::*, transform::TransformSystem};

use crate::game_state::AppState;
use crate::network::protocol::RemotePlayer;

/// Resource holding the loaded character GLTF handle.
#[derive(Resource)]
pub struct CharacterGltfHandle(pub Handle<Gltf>);

/// Resource holding the processed character assets (set after GLTF loads).
#[derive(Resource)]
pub struct CharacterAssets {
    pub scene: Handle<Scene>,
    pub animations: Vec<Handle<AnimationClip>>,
    pub animation_graph: Handle<AnimationGraph>,
    pub idle_index: AnimationNodeIndex,
    pub walk_index: AnimationNodeIndex,
}

/// Component to mark that a character model needs animation setup.
#[derive(Component)]
pub struct NeedsAnimationSetup;

/// Component to track current animation state.
#[derive(Component)]
pub struct CharacterAnimationState {
    pub is_walking: bool,
    /// Time when walking was last detected (for decay)
    pub last_walk_time: f32,
    /// Track the last animation we played to detect changes
    pub last_was_walking: Option<bool>,
}

impl Default for CharacterAnimationState {
    fn default() -> Self {
        Self {
            is_walking: false,
            last_walk_time: 0.0,
            last_was_walking: None, // None means we haven't started any animation yet
        }
    }
}

/// Tracks if animation has been initialized for this character.
#[derive(Component)]
pub struct AnimationInitialized;

/// Links a character root entity to its animation player entity.
#[derive(Component)]
pub struct CharacterAnimationLink(pub Entity);

/// Links a character root entity to its head bone entity for head tracking.
#[derive(Component)]
pub struct CharacterHeadLink(pub Entity);

pub struct CharacterPlugin;

/// How long walking state persists after movement stops (in seconds).
const WALK_DECAY_TIME: f32 = 0.15;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, start_loading_character)
            // Process GLTF in all states so it's ready before InGame
            .add_systems(Update, process_loaded_gltf)
            .add_systems(
                Update,
                (
                    attach_model_to_players_without_model,
                    decay_walking_state,
                    setup_character_animation_graph,
                    setup_head_bone_link,
                    start_character_animations,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            // Run head rotation after animation systems
            .add_systems(
                PostUpdate,
                update_head_rotation
                    .after(Animation)
                    .before(TransformSystem::TransformPropagate)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Decay walking state after movement stops.
fn decay_walking_state(time: Res<Time>, mut query: Query<&mut CharacterAnimationState>) {
    for mut anim_state in query.iter_mut() {
        if anim_state.is_walking {
            anim_state.last_walk_time += time.delta_secs();
            if anim_state.last_walk_time > WALK_DECAY_TIME {
                anim_state.is_walking = false;
                anim_state.last_walk_time = 0.0;
            }
        }
    }
}

fn start_loading_character(mut commands: Commands, asset_server: Res<AssetServer>) {
    let gltf_handle: Handle<Gltf> = asset_server.load("characters/character-a.glb");
    commands.insert_resource(CharacterGltfHandle(gltf_handle));
    info!("Character GLTF loading started");
}

/// Process the GLTF once it's loaded and extract scene + animations.
fn process_loaded_gltf(
    mut commands: Commands,
    gltf_handle: Option<Res<CharacterGltfHandle>>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    existing_assets: Option<Res<CharacterAssets>>,
) {
    // Skip if already processed
    if existing_assets.is_some() {
        return;
    }

    let Some(handle) = gltf_handle else {
        return;
    };

    let Some(gltf) = gltfs.get(&handle.0) else {
        return;
    };

    // Get the default scene
    let scene = gltf.default_scene.clone().unwrap_or_else(|| {
        gltf.scenes.first().cloned().expect("GLTF has no scenes")
    });

    // Get all animations
    let animations: Vec<Handle<AnimationClip>> = gltf.animations.clone();

    info!(
        "Character GLTF loaded: {} animations found",
        animations.len()
    );

    // Log animation names
    for (i, _) in animations.iter().enumerate() {
        if let Some(name) = gltf.named_animations.iter().find(|(_, h)| {
            // Compare handles by checking if they point to same asset
            animations.get(i).map(|a| a == *h).unwrap_or(false)
        }) {
            info!("  Animation {}: {}", i, name.0);
        } else {
            info!("  Animation {}: (unnamed)", i);
        }
    }

    // Create animation graph with static (index 0) for idle pose and walk (index 2)
    // Using "static" instead of "idle" because idle may not animate legs
    let mut graph = AnimationGraph::new();
    let (idle_index, walk_index) = if animations.len() >= 3 {
        let idle = graph.add_clip(animations[0].clone(), 1.0, graph.root); // "static" - full body neutral pose
        let walk = graph.add_clip(animations[2].clone(), 1.0, graph.root); // "walk"
        (idle, walk)
    } else if !animations.is_empty() {
        let idx = graph.add_clip(animations[0].clone(), 1.0, graph.root);
        (idx, idx)
    } else {
        warn!("No animations found in character GLTF!");
        (graph.root, graph.root)
    };

    let animation_graph = graphs.add(graph);

    commands.insert_resource(CharacterAssets {
        scene,
        animations,
        animation_graph,
        idle_index,
        walk_index,
    });

    info!("Character assets processed and ready");
}

/// Attach character model to remote players that were spawned without one.
fn attach_model_to_players_without_model(
    mut commands: Commands,
    character_assets: Option<Res<CharacterAssets>>,
    query: Query<
        (Entity, &Transform),
        (
            With<RemotePlayer>,
            With<CharacterAnimationState>,
            Without<NeedsAnimationSetup>,
            Without<AnimationInitialized>,
        ),
    >,
    scene_query: Query<&SceneRoot>,
) {
    let Some(assets) = character_assets else {
        return;
    };

    for (entity, transform) in query.iter() {
        // Check if this entity already has a scene
        if scene_query.get(entity).is_ok() {
            continue;
        }

        info!("Attaching character model to player {:?} at {:?}", entity, transform.translation);
        commands.entity(entity).insert((
            SceneRoot(assets.scene.clone()),
            NeedsAnimationSetup,
            // Update scale if not already set
            Transform {
                scale: Vec3::splat(1.0),
                ..*transform
            },
        ));
    }
}

/// Phase 1: Find AnimationPlayer in hierarchy and add the animation graph.
fn setup_character_animation_graph(
    mut commands: Commands,
    character_assets: Option<Res<CharacterAssets>>,
    query: Query<Entity, (With<NeedsAnimationSetup>, Without<AnimationInitialized>)>,
    children_query: Query<&Children>,
    animation_player_query: Query<Entity, With<AnimationPlayer>>,
) {
    let Some(assets) = character_assets else {
        return;
    };

    for root_entity in query.iter() {
        // Find the AnimationPlayer entity in the hierarchy
        if let Some(anim_entity) =
            find_entity_with_component(root_entity, &children_query, &animation_player_query)
        {
            info!(
                "Found AnimationPlayer for character {:?} at entity {:?}",
                root_entity, anim_entity
            );

            // Add the animation graph and transitions to the animation player entity
            commands
                .entity(anim_entity)
                .insert(AnimationGraphHandle(assets.animation_graph.clone()))
                .insert(AnimationTransitions::new());

            // Mark as initialized and store the link to animation player
            commands
                .entity(root_entity)
                .insert(AnimationInitialized)
                .insert(CharacterAnimationLink(anim_entity))
                .remove::<NeedsAnimationSetup>();
        }
    }
}

/// Phase 2: Start and update animations based on movement state.
fn start_character_animations(
    character_assets: Option<Res<CharacterAssets>>,
    mut character_query: Query<(&CharacterAnimationLink, &mut CharacterAnimationState), With<AnimationInitialized>>,
    mut animation_query: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    let Some(assets) = character_assets else {
        return;
    };

    // Skip if no animations
    if assets.animations.is_empty() {
        return;
    }

    for (link, mut anim_state) in character_query.iter_mut() {
        if let Ok((mut player, mut transitions)) = animation_query.get_mut(link.0) {
            // Detect state change (None means first time, always trigger)
            let state_changed = anim_state.last_was_walking != Some(anim_state.is_walking);

            // Choose animation based on movement state
            let target_anim = if anim_state.is_walking {
                assets.walk_index
            } else {
                assets.idle_index
            };

            // Switch animation when state changes
            if state_changed {
                anim_state.last_was_walking = Some(anim_state.is_walking);
                // Use transitions for smooth blending (150ms crossfade)
                transitions
                    .play(&mut player, target_anim, std::time::Duration::from_millis(150))
                    .repeat();
            }
        }
    }
}

/// Recursively finds an entity with the specified component in the hierarchy.
fn find_entity_with_component<T: Component>(
    entity: Entity,
    children_query: &Query<&Children>,
    component_query: &Query<Entity, With<T>>,
) -> Option<Entity> {
    // Check if this entity has the component
    if component_query.get(entity).is_ok() {
        return Some(entity);
    }

    // Check children recursively
    if let Ok(children) = children_query.get(entity) {
        for &child in children.iter() {
            if let Some(result) =
                find_entity_with_component(child, children_query, component_query)
            {
                return Some(result);
            }
        }
    }

    None
}

/// Recursively finds an entity with the specified name in the hierarchy.
fn find_entity_by_name(
    entity: Entity,
    name: &str,
    children_query: &Query<&Children>,
    name_query: &Query<&Name>,
) -> Option<Entity> {
    // Check if this entity has the matching name
    if let Ok(entity_name) = name_query.get(entity) {
        if entity_name.as_str() == name {
            return Some(entity);
        }
    }

    // Check children recursively
    if let Ok(children) = children_query.get(entity) {
        for &child in children.iter() {
            if let Some(result) = find_entity_by_name(child, name, children_query, name_query) {
                return Some(result);
            }
        }
    }

    None
}

/// Find and link the head bone for characters that have been initialized.
fn setup_head_bone_link(
    mut commands: Commands,
    query: Query<Entity, (With<AnimationInitialized>, Without<CharacterHeadLink>)>,
    children_query: Query<&Children>,
    name_query: Query<&Name>,
) {
    for root_entity in query.iter() {
        // Try common head bone names
        let head_names = ["Head", "head", "mixamorig:Head", "Bone.Head"];

        for head_name in head_names {
            if let Some(head_entity) =
                find_entity_by_name(root_entity, head_name, &children_query, &name_query)
            {
                info!("Found head bone '{}' for character {:?}", head_name, root_entity);
                commands
                    .entity(root_entity)
                    .insert(CharacterHeadLink(head_entity));
                break;
            }
        }
    }
}

/// Stores the interpolated pitch for smooth head movement.
#[derive(Component, Default)]
pub struct HeadPitch {
    pub current: f32,
}

/// Update head rotation based on pitch from NetworkTransform.
/// This runs after transform propagation.
fn update_head_rotation(
    mut commands: Commands,
    character_query: Query<(Entity, &CharacterHeadLink, &crate::network::protocol::NetworkTransform), Without<HeadPitch>>,
    mut head_query: Query<(Entity, &CharacterHeadLink, &crate::network::protocol::NetworkTransform, &mut HeadPitch)>,
    mut transform_query: Query<&mut Transform>,
    time: Res<Time>,
) {
    // Add HeadPitch component to new characters
    for (entity, _, _) in character_query.iter() {
        commands.entity(entity).insert(HeadPitch::default());
    }

    // Update head rotation for characters with HeadPitch
    for (_entity, head_link, net_transform, mut head_pitch) in head_query.iter_mut() {
        // Smoothly interpolate pitch
        let target_pitch = net_transform.target_pitch;
        head_pitch.current = head_pitch.current + (target_pitch - head_pitch.current) * time.delta_secs() * 10.0;

        if let Ok(mut head_transform) = transform_query.get_mut(head_link.0) {
            // Get the current rotation from animation and apply pitch on top
            let current_rot = head_transform.rotation;
            let pitch_rotation = Quat::from_rotation_x(-head_pitch.current);
            head_transform.rotation = current_rot * pitch_rotation;
        }
    }
}
