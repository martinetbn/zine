use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use scrap::Display;

use super::capture::{CaptureSource, CaptureSourceType};
use super::window_capture::{enumerate_windows, WindowInfo};

/// Resource tracking the share UI state.
#[derive(Resource, Default)]
pub struct ShareUIState {
    pub selected_tab: ShareTab,
    pub selected_source: Option<usize>,
    pub available_screens: Vec<ScreenInfo>,
    pub available_windows: Vec<WindowInfo>,
    pub needs_refresh: bool,
    pub last_rendered_tab: Option<ShareTab>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum ShareTab {
    #[default]
    Screens,
    Windows,
}

#[derive(Clone)]
pub struct ScreenInfo {
    pub name: String,
    pub index: usize,
}

/// Resource marker for when UI is open.
#[derive(Resource)]
pub struct ShareUIRoot(pub Entity);

// UI Components
#[derive(Component)]
pub struct ShareUIContainer;

#[derive(Component)]
pub struct TabButton(pub ShareTab);

#[derive(Component)]
pub struct SourceButton(pub usize);

#[derive(Component)]
pub struct CancelButton;

#[derive(Component)]
pub struct ShareButton;

#[derive(Component)]
pub struct SourceListContainer;

// Colors
const BG_COLOR: Color = Color::srgba(0.1, 0.1, 0.1, 0.95);
const TAB_NORMAL: Color = Color::srgb(0.2, 0.2, 0.2);
const TAB_SELECTED: Color = Color::srgb(0.3, 0.5, 0.3);
const BUTTON_NORMAL: Color = Color::srgb(0.25, 0.25, 0.25);
const BUTTON_HOVER: Color = Color::srgb(0.35, 0.35, 0.35);
const SOURCE_SELECTED: Color = Color::srgb(0.2, 0.4, 0.6);
const CANCEL_COLOR: Color = Color::srgb(0.5, 0.3, 0.3);
const SHARE_COLOR: Color = Color::srgb(0.3, 0.5, 0.3);

pub fn setup_share_ui(commands: &mut Commands) {
    // Release cursor for UI interaction
    // (handled separately)

    let root = commands
        .spawn((
            ShareUIContainer,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            GlobalZIndex(100),
        ))
        .with_children(|parent| {
            // Modal container
            parent
                .spawn((
                    Node {
                        width: Val::Px(500.0),
                        height: Val::Px(400.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(20.0)),
                        ..default()
                    },
                    BackgroundColor(BG_COLOR),
                ))
                .with_children(|modal| {
                    // Title
                    modal.spawn((
                        Text::new("Share Screen"),
                        TextFont {
                            font_size: 24.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        Node {
                            margin: UiRect::bottom(Val::Px(15.0)),
                            ..default()
                        },
                    ));

                    // Tab bar
                    modal
                        .spawn((Node {
                            flex_direction: FlexDirection::Row,
                            margin: UiRect::bottom(Val::Px(15.0)),
                            ..default()
                        },))
                        .with_children(|tabs| {
                            // Screens tab
                            spawn_tab_button(tabs, "Screens", ShareTab::Screens, true);
                            // Windows tab
                            spawn_tab_button(tabs, "Windows", ShareTab::Windows, false);
                        });

                    // Source list container
                    modal.spawn((
                        SourceListContainer,
                        Node {
                            flex_direction: FlexDirection::Column,
                            flex_grow: 1.0,
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                    ));

                    // Bottom buttons
                    modal
                        .spawn((Node {
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::FlexEnd,
                            margin: UiRect::top(Val::Px(15.0)),
                            column_gap: Val::Px(10.0),
                            ..default()
                        },))
                        .with_children(|buttons| {
                            // Cancel button
                            buttons
                                .spawn((
                                    CancelButton,
                                    Button,
                                    Node {
                                        width: Val::Px(100.0),
                                        height: Val::Px(40.0),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(CANCEL_COLOR),
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new("Cancel"),
                                        TextFont {
                                            font_size: 16.0,
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                    ));
                                });

                            // Share button
                            buttons
                                .spawn((
                                    ShareButton,
                                    Button,
                                    Node {
                                        width: Val::Px(100.0),
                                        height: Val::Px(40.0),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(SHARE_COLOR),
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new("Share"),
                                        TextFont {
                                            font_size: 16.0,
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                    ));
                                });
                        });
                });
        })
        .id();

    commands.insert_resource(ShareUIRoot(root));

    info!("Share UI opened, marking state for refresh");
}

/// Call this when opening the UI to ensure the list refreshes
pub fn mark_share_ui_needs_refresh(mut state: ResMut<ShareUIState>) {
    state.needs_refresh = true;
    state.selected_source = None;
}

fn spawn_tab_button(parent: &mut ChildBuilder, label: &str, tab: ShareTab, selected: bool) {
    parent
        .spawn((
            TabButton(tab),
            Button,
            Node {
                width: Val::Px(120.0),
                height: Val::Px(35.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                margin: UiRect::right(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(if selected { TAB_SELECTED } else { TAB_NORMAL }),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

pub fn cleanup_share_ui(mut commands: Commands, root: Option<Res<ShareUIRoot>>) {
    if let Some(root) = root {
        commands.entity(root.0).despawn_recursive();
        commands.remove_resource::<ShareUIRoot>();
    }
}

pub fn handle_share_ui_interaction(
    mut commands: Commands,
    mut state: ResMut<ShareUIState>,
    mut windows: Query<&mut Window>,
    root: Option<Res<ShareUIRoot>>,
    tab_query: Query<(&Interaction, &TabButton), Changed<Interaction>>,
    source_query: Query<(&Interaction, &SourceButton), Changed<Interaction>>,
    cancel_query: Query<&Interaction, (Changed<Interaction>, With<CancelButton>)>,
    share_query: Query<&Interaction, (Changed<Interaction>, With<ShareButton>)>,
    mut tab_buttons: Query<(&TabButton, &mut BackgroundColor), Without<SourceButton>>,
    mut source_buttons: Query<(&SourceButton, &mut BackgroundColor), Without<TabButton>>,
    mut capture_events: EventWriter<CaptureSource>,
) {
    let Some(root) = root else { return };

    // Release cursor when UI is open
    if let Ok(mut window) = windows.get_single_mut() {
        if window.cursor_options.grab_mode != CursorGrabMode::None {
            window.cursor_options.grab_mode = CursorGrabMode::None;
            window.cursor_options.visible = true;
        }
    }

    // Handle tab clicks
    for (interaction, tab_button) in tab_query.iter() {
        if *interaction == Interaction::Pressed {
            state.selected_tab = tab_button.0;
            state.selected_source = None;

            // Update tab visuals
            for (tab, mut bg) in tab_buttons.iter_mut() {
                bg.0 = if tab.0 == state.selected_tab {
                    TAB_SELECTED
                } else {
                    TAB_NORMAL
                };
            }
        }
    }

    // Handle source selection
    for (interaction, source_button) in source_query.iter() {
        if *interaction == Interaction::Pressed {
            state.selected_source = Some(source_button.0);

            // Update source visuals
            for (source, mut bg) in source_buttons.iter_mut() {
                bg.0 = if Some(source.0) == state.selected_source {
                    SOURCE_SELECTED
                } else {
                    BUTTON_NORMAL
                };
            }
        }
    }

    // Handle cancel
    for interaction in cancel_query.iter() {
        if *interaction == Interaction::Pressed {
            commands.entity(root.0).despawn_recursive();
            commands.remove_resource::<ShareUIRoot>();

            // Re-grab cursor
            if let Ok(mut window) = windows.get_single_mut() {
                window.cursor_options.grab_mode = CursorGrabMode::Locked;
                window.cursor_options.visible = false;
            }
            return;
        }
    }

    // Handle share
    for interaction in share_query.iter() {
        if *interaction == Interaction::Pressed {
            if let Some(source_idx) = state.selected_source {
                // Determine capture source based on selected tab
                let capture_source = match state.selected_tab {
                    ShareTab::Screens => {
                        if let Some(screen) = state.available_screens.get(source_idx) {
                            info!("Starting display capture for screen {}", screen.index);
                            Some(CaptureSourceType::Display(screen.index))
                        } else {
                            None
                        }
                    }
                    ShareTab::Windows => {
                        if let Some(window) = state.available_windows.get(source_idx) {
                            info!("Starting window capture for: {} (hwnd: {})", window.title, window.hwnd);
                            Some(CaptureSourceType::Window(window.hwnd))
                        } else {
                            None
                        }
                    }
                };

                if let Some(source) = capture_source {
                    capture_events.send(CaptureSource { source });

                    // Close UI
                    commands.entity(root.0).despawn_recursive();
                    commands.remove_resource::<ShareUIRoot>();

                    // Re-grab cursor
                    if let Ok(mut window) = windows.get_single_mut() {
                        window.cursor_options.grab_mode = CursorGrabMode::Locked;
                        window.cursor_options.visible = false;
                    }
                    return;
                }
            }
        }
    }
}

pub fn update_source_list(
    mut commands: Commands,
    mut state: ResMut<ShareUIState>,
    list_container: Query<Entity, With<SourceListContainer>>,
) {
    // Check if tab changed
    let tab_changed = state.last_rendered_tab != Some(state.selected_tab);

    // Determine if we need to refresh based on current tab
    let needs_data_refresh = match state.selected_tab {
        ShareTab::Screens => state.available_screens.is_empty() || state.needs_refresh,
        ShareTab::Windows => state.available_windows.is_empty() || state.needs_refresh || tab_changed,
    };

    // Only update when data is empty, needs refresh, or tab changed
    if !needs_data_refresh && !tab_changed {
        return;
    }

    state.needs_refresh = false;
    state.last_rendered_tab = Some(state.selected_tab);

    // Enumerate data based on selected tab
    match state.selected_tab {
        ShareTab::Screens => {
            if state.available_screens.is_empty() {
                // Enumerate displays
                match Display::all() {
                    Ok(displays) => {
                        info!("Found {} displays", displays.len());
                        for (i, disp) in displays.iter().enumerate() {
                            let w = disp.width();
                            let h = disp.height();
                            let name = format!("Display {} ({}x{})", i + 1, w, h);
                            info!("  Display {}: {}x{}", i, w, h);
                            state.available_screens.push(ScreenInfo {
                                name,
                                index: i,
                            });
                        }
                    }
                    Err(e) => {
                        error!("Failed to enumerate displays: {}", e);
                    }
                }
            }
        }
        ShareTab::Windows => {
            // Always refresh windows list when switching to this tab
            state.available_windows.clear();
            info!("About to enumerate windows...");
            let windows = enumerate_windows();
            info!("enumerate_windows returned {} windows", windows.len());
            for win in windows {
                info!("  Window: {} (hwnd: {})", win.title, win.hwnd);
                state.available_windows.push(win);
            }
        }
    }

    // Populate the list
    if let Ok(container) = list_container.get_single() {
        // Clear all children of the container (buttons and placeholder text)
        commands.entity(container).despawn_descendants();

        // Add source buttons
        commands.entity(container).with_children(|parent| {
            if state.selected_tab == ShareTab::Screens {
                for screen in &state.available_screens {
                    spawn_source_button(parent, &screen.name, screen.index);
                }

                if state.available_screens.is_empty() {
                    parent.spawn((
                        Text::new("No displays found"),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.5, 0.5, 0.5)),
                        Node {
                            margin: UiRect::all(Val::Px(10.0)),
                            ..default()
                        },
                    ));
                }
            } else {
                // Windows tab
                for (idx, window) in state.available_windows.iter().enumerate() {
                    // Truncate long window titles
                    let title = if window.title.len() > 50 {
                        format!("{}...", &window.title[..47])
                    } else {
                        window.title.clone()
                    };
                    spawn_source_button(parent, &title, idx);
                }

                if state.available_windows.is_empty() {
                    parent.spawn((
                        Text::new("No capturable windows found"),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.5, 0.5, 0.5)),
                        Node {
                            margin: UiRect::all(Val::Px(10.0)),
                            ..default()
                        },
                    ));
                }
            }
        });
    }
}

fn spawn_source_button(parent: &mut ChildBuilder, label: &str, index: usize) {
    parent
        .spawn((
            SourceButton(index),
            Button,
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(45.0),
                padding: UiRect::all(Val::Px(10.0)),
                margin: UiRect::all(Val::Px(2.0)),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BUTTON_NORMAL),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}
