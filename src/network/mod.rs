pub mod client;
pub mod discovery;
pub mod server;

use bevy::prelude::*;

pub use discovery::{DiscoveredSessions, LanSession, SelectedSession};

use crate::game_state::AppState;
use discovery::{
    broadcast_session, cleanup_broadcast, cleanup_listener, listen_for_sessions, setup_broadcast,
    setup_listener,
};

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        // Initialize discovery resources
        app.init_resource::<DiscoveredSessions>();

        // Server plugin
        server::server_plugin(app);

        // Client plugin
        client::client_plugin(app);

        // Discovery - host broadcasts
        app.add_systems(OnEnter(AppState::Hosting), setup_broadcast)
            .add_systems(OnExit(AppState::InGame), cleanup_broadcast)
            .add_systems(
                Update,
                broadcast_session.run_if(in_state(AppState::Hosting).or(in_state(AppState::InGame))),
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
