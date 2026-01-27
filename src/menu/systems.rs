use bevy::prelude::*;

use super::components::*;
use super::styles::*;
use crate::game_state::AppState;
use crate::network::DiscoveredSessions;

pub fn setup_main_menu(mut commands: Commands) {
    // Spawn menu camera for UI rendering
    commands.spawn((MenuCamera, Camera2d));

    // Root container
    commands
        .spawn((
            MainMenuRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
        ))
        .with_children(|parent| {
            // Title
            parent.spawn((
                Text::new("ZINE"),
                title_text_style(),
                TextColor(TITLE_TEXT_COLOR),
                Node {
                    margin: UiRect::bottom(Val::Px(50.0)),
                    ..default()
                },
            ));

            // Host button
            parent
                .spawn((
                    HostButton,
                    Button,
                    button_style(),
                    BackgroundColor(NORMAL_BUTTON),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Host Game"),
                        button_text_style(),
                        TextColor(BUTTON_TEXT_COLOR),
                    ));
                });

            // Join button
            parent
                .spawn((
                    JoinButton,
                    Button,
                    button_style(),
                    BackgroundColor(NORMAL_BUTTON),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Join Game"),
                        button_text_style(),
                        TextColor(BUTTON_TEXT_COLOR),
                    ));
                });
        });
}

pub fn cleanup_main_menu(mut commands: Commands, query: Query<Entity, With<MainMenuRoot>>) {
    for entity in query.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

pub fn cleanup_menu_camera(mut commands: Commands, query: Query<Entity, With<MenuCamera>>) {
    for entity in query.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

pub fn setup_browser(mut commands: Commands) {
    // Root container for browser
    commands
        .spawn((
            BrowserRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(40.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
        ))
        .with_children(|parent| {
            // Title
            parent.spawn((
                Text::new("Join Game"),
                title_text_style(),
                TextColor(TITLE_TEXT_COLOR),
                Node {
                    margin: UiRect::bottom(Val::Px(30.0)),
                    ..default()
                },
            ));

            // Searching text
            parent.spawn((
                SearchingText,
                Text::new("Searching for games..."),
                button_text_style(),
                TextColor(Color::srgb(0.6, 0.6, 0.6)),
                Node {
                    margin: UiRect::bottom(Val::Px(20.0)),
                    ..default()
                },
            ));

            // Session list container
            parent.spawn((
                SessionList,
                Node {
                    width: Val::Px(400.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ));

            // Back button
            parent
                .spawn((
                    BackButton,
                    Button,
                    Node {
                        margin: UiRect::top(Val::Px(30.0)),
                        ..button_style()
                    },
                    BackgroundColor(NORMAL_BUTTON),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Back"),
                        button_text_style(),
                        TextColor(BUTTON_TEXT_COLOR),
                    ));
                });
        });
}

pub fn cleanup_browser(mut commands: Commands, query: Query<Entity, With<BrowserRoot>>) {
    for entity in query.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

pub fn button_interaction(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, mut color) in interaction_query.iter_mut() {
        match *interaction {
            Interaction::Pressed => {
                *color = PRESSED_BUTTON.into();
            }
            Interaction::Hovered => {
                *color = HOVERED_BUTTON.into();
            }
            Interaction::None => {
                *color = NORMAL_BUTTON.into();
            }
        }
    }
}

pub fn handle_host_click(
    interaction_query: Query<&Interaction, (Changed<Interaction>, With<HostButton>)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in interaction_query.iter() {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::Hosting);
        }
    }
}

pub fn handle_join_click(
    interaction_query: Query<&Interaction, (Changed<Interaction>, With<JoinButton>)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in interaction_query.iter() {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::Browsing);
        }
    }
}

pub fn handle_back_click(
    interaction_query: Query<&Interaction, (Changed<Interaction>, With<BackButton>)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in interaction_query.iter() {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::MainMenu);
        }
    }
}

pub fn update_session_list(
    mut commands: Commands,
    sessions: Res<DiscoveredSessions>,
    session_list_query: Query<Entity, With<SessionList>>,
    existing_entries: Query<Entity, With<SessionEntry>>,
    mut searching_text: Query<&mut TextColor, With<SearchingText>>,
) {
    if !sessions.is_changed() {
        return;
    }

    // Remove old entries
    for entity in existing_entries.iter() {
        commands.entity(entity).despawn_recursive();
    }

    // Update searching text visibility
    if let Ok(mut color) = searching_text.get_single_mut() {
        if sessions.0.is_empty() {
            color.0 = Color::srgb(0.6, 0.6, 0.6);
        } else {
            color.0 = Color::NONE;
        }
    }

    // Add new entries
    if let Ok(session_list) = session_list_query.get_single() {
        commands.entity(session_list).with_children(|parent| {
            for (index, session) in sessions.0.iter().enumerate() {
                parent
                    .spawn((
                        SessionEntry(index),
                        Button,
                        button_style(),
                        BackgroundColor(NORMAL_BUTTON),
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            Text::new(format!("{} ({} players)", session.name, session.player_count)),
                            button_text_style(),
                            TextColor(BUTTON_TEXT_COLOR),
                        ));
                    });
            }
        });
    }
}

pub fn handle_session_click(
    interaction_query: Query<(&Interaction, &SessionEntry), Changed<Interaction>>,
    sessions: Res<DiscoveredSessions>,
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
) {
    for (interaction, entry) in interaction_query.iter() {
        if *interaction == Interaction::Pressed {
            if let Some(session) = sessions.0.get(entry.0) {
                // Store the selected session for connection
                commands.insert_resource(crate::network::SelectedSession(session.clone()));
                next_state.set(AppState::Connecting);
            }
        }
    }
}

pub fn release_cursor(mut windows: Query<&mut Window>) {
    if let Ok(mut window) = windows.get_single_mut() {
        window.cursor_options.grab_mode = bevy::window::CursorGrabMode::None;
        window.cursor_options.visible = true;
    }
}
