pub mod client;
pub mod discovery;
pub mod protocol;
pub mod server;

use bevy::prelude::*;

pub use client::ReceivedScreenFrame;
pub use discovery::{DiscoveredSessions, LanSession, SelectedSession};
pub use protocol::{LocalPlayerId, RemotePlayer, RemotePlayers};

use crate::game_state::AppState;
use client::{interpolate_remote_players, update_remote_player_visuals};
use discovery::{
    broadcast_session, cleanup_broadcast, cleanup_listener, listen_for_sessions, setup_broadcast,
    setup_listener,
};

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        // Initialize discovery resources
        app.init_resource::<DiscoveredSessions>();

        // Register screen frame event
        app.add_event::<ReceivedScreenFrame>();

        // Server plugin
        server::server_plugin(app);

        // Client plugin
        client::client_plugin(app);

        // Remote player visuals (for both host and client)
        app.add_systems(
            Update,
            (update_remote_player_visuals, interpolate_remote_players)
                .run_if(in_state(AppState::InGame)),
        );

        // Host also needs RemotePlayers to see clients
        app.add_systems(OnEnter(AppState::Hosting), setup_host_remote_players);
        app.add_systems(
            Update,
            sync_host_remote_players
                .run_if(in_state(AppState::InGame).and(resource_exists::<server::GameServer>)),
        );

        // Discovery - host broadcasts
        app.add_systems(OnEnter(AppState::Hosting), setup_broadcast)
            .add_systems(OnExit(AppState::InGame), cleanup_broadcast)
            .add_systems(
                Update,
                broadcast_session
                    .run_if(in_state(AppState::Hosting).or(in_state(AppState::InGame))),
            );

        // Discovery - client listens
        app.add_systems(OnEnter(AppState::Browsing), setup_listener)
            .add_systems(OnExit(AppState::Browsing), cleanup_listener)
            .add_systems(
                Update,
                listen_for_sessions.run_if(in_state(AppState::Browsing)),
            );
    }
}

fn setup_host_remote_players(mut commands: Commands) {
    commands.init_resource::<RemotePlayers>();
}

fn sync_host_remote_players(
    server: Res<server::GameServer>,
    local_id: Res<LocalPlayerId>,
    mut remote_players: ResMut<RemotePlayers>,
) {
    // Host sees all players except themselves
    remote_players.players = server
        .player_states
        .values()
        .filter(|p| p.id != local_id.0)
        .cloned()
        .collect();
}
