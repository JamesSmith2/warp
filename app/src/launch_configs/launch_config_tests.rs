use std::path::PathBuf;

use crate::{
    app_state::{
        AppState, BranchSnapshot, LeafContents, LeafSnapshot, NotebookPaneSnapshot, PaneFlex,
        PaneNodeSnapshot, SplitDirection, TabSnapshot, TerminalPaneSnapshot, WindowSnapshot,
        WorkspaceGroupSnapshot,
    },
    drive::OpenWarpDriveObjectSettings,
    tab::SelectedTabColor,
    themes::theme::AnsiColorIdentifier,
};

use super::{LaunchConfig, PaneMode, PaneTemplateType};

fn single_tab_snapshot(root: PaneNodeSnapshot) -> AppState {
    AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                root,
                left_panel: None,
                right_panel: None,
            }],
            active_tab_index: 0,
            workspace_groups: vec![],
            active_workspace_group_index: 0,
            bounds: None,
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            fullscreen_state: Default::default(),
            left_panel_width: None,
            right_panel_width: None,
            agent_management_filters: None,
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    }
}

fn multi_tab_snapshot(active_tab_index: usize, tabs: Vec<TabSnapshot>) -> AppState {
    AppState {
        windows: vec![WindowSnapshot {
            tabs,
            active_tab_index,
            workspace_groups: vec![],
            active_workspace_group_index: 0,
            bounds: None,
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            fullscreen_state: Default::default(),
            left_panel_width: None,
            right_panel_width: None,
            agent_management_filters: None,
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    }
}

fn terminal_tab_snapshot(title: &str, cwd: &str) -> TabSnapshot {
    TabSnapshot {
        custom_title: Some(title.to_string()),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: true,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![],
                cwd: Some(cwd.to_string()),
                is_active: true,
                is_read_only: false,
                shell_launch_data: None,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                active_conversation_id: None,
            }),
        }),
        left_panel: None,
        right_panel: None,
    }
}

#[test]
fn test_config_from_snapshot_flattens_single_pane() {
    // If only one pane of the branch can be saved into a launch configuration, it should
    // be flattened to a single leaf.

    let state = single_tab_snapshot(PaneNodeSnapshot::Branch(BranchSnapshot {
        direction: SplitDirection::Vertical,
        children: vec![
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Notebook(NotebookPaneSnapshot::CloudNotebook {
                        notebook_id: None,
                        settings: OpenWarpDriveObjectSettings::default(),
                    }),
                }),
            ),
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Terminal(TerminalPaneSnapshot {
                        uuid: vec![],
                        cwd: Some("/some/dir".into()),
                        is_active: true,
                        is_read_only: false,
                        shell_launch_data: None,
                        input_config: None,
                        llm_model_override: None,
                        active_profile_id: None,
                        conversation_ids_to_restore: vec![],
                        active_conversation_id: None,
                    }),
                }),
            ),
        ],
    }));

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert_eq!(
        template.windows[0].tabs[0].layout,
        PaneTemplateType::PaneTemplate {
            is_focused: Some(true),
            cwd: PathBuf::from("/some/dir"),
            commands: vec![],
            pane_mode: PaneMode::Terminal,
            shell: None,
        },
    )
}

#[test]
fn test_config_from_snapshot_filters_panes() {
    let state = single_tab_snapshot(PaneNodeSnapshot::Branch(BranchSnapshot {
        direction: SplitDirection::Vertical,
        children: vec![
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Terminal(TerminalPaneSnapshot {
                        uuid: vec![],
                        cwd: Some("/path/to/dir".into()),
                        is_active: true,
                        is_read_only: false,
                        shell_launch_data: None,
                        input_config: None,
                        llm_model_override: None,
                        active_profile_id: None,
                        conversation_ids_to_restore: vec![],
                        active_conversation_id: None,
                    }),
                }),
            ),
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: false,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Notebook(NotebookPaneSnapshot::CloudNotebook {
                        notebook_id: None,
                        settings: OpenWarpDriveObjectSettings::default(),
                    }),
                }),
            ),
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: false,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Terminal(TerminalPaneSnapshot {
                        uuid: vec![],
                        cwd: Some("/some/dir".into()),
                        is_active: true,
                        is_read_only: false,
                        shell_launch_data: None,
                        input_config: None,
                        llm_model_override: None,
                        active_profile_id: None,
                        conversation_ids_to_restore: vec![],
                        active_conversation_id: None,
                    }),
                }),
            ),
        ],
    }));

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert_eq!(
        template.windows[0].tabs[0].layout,
        PaneTemplateType::PaneBranchTemplate {
            split_direction: SplitDirection::Vertical.into(),
            panes: vec![
                PaneTemplateType::PaneTemplate {
                    is_focused: Some(true),
                    cwd: PathBuf::from("/path/to/dir"),
                    commands: vec![],
                    pane_mode: PaneMode::Terminal,
                    shell: None,
                },
                PaneTemplateType::PaneTemplate {
                    is_focused: Some(false),
                    cwd: PathBuf::from("/some/dir"),
                    commands: vec![],
                    pane_mode: PaneMode::Terminal,
                    shell: None,
                },
            ]
        }
    )
}

#[test]
fn test_config_from_snapshot_filters_tabs() {
    // If no panes of a tab are valid, it's filtered out entirely.

    let state = single_tab_snapshot(PaneNodeSnapshot::Branch(BranchSnapshot {
        direction: SplitDirection::Vertical,
        children: vec![(
            PaneFlex(1.),
            PaneNodeSnapshot::Leaf(LeafSnapshot {
                is_focused: true,
                custom_vertical_tabs_title: None,
                contents: LeafContents::Notebook(NotebookPaneSnapshot::CloudNotebook {
                    notebook_id: None,
                    settings: OpenWarpDriveObjectSettings::default(),
                }),
            }),
        )],
    }));

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert!(template.windows[0].tabs.is_empty())
}

#[test]
fn test_config_with_active_tab_index() {
    let state = multi_tab_snapshot(
        1,
        vec![
            TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                root: PaneNodeSnapshot::Branch(BranchSnapshot {
                    direction: SplitDirection::Vertical,
                    children: vec![(
                        PaneFlex(1.),
                        PaneNodeSnapshot::Leaf(LeafSnapshot {
                            is_focused: true,
                            custom_vertical_tabs_title: None,
                            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                                uuid: vec![],
                                cwd: Some("/path/to/dir".into()),
                                is_active: true,
                                is_read_only: false,
                                shell_launch_data: None,
                                input_config: None,
                                llm_model_override: None,
                                active_profile_id: None,
                                conversation_ids_to_restore: vec![],
                                active_conversation_id: None,
                            }),
                        }),
                    )],
                }),
                left_panel: None,
                right_panel: None
            };
            3
        ],
    );

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert_eq!(template.windows[0].active_tab_index, Some(1))
}

#[test]
fn test_config_with_active_tab_index_and_filtered_tabs() {
    let state = multi_tab_snapshot(
        1,
        vec![
            TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                root: PaneNodeSnapshot::Branch(BranchSnapshot {
                    direction: SplitDirection::Vertical,
                    children: vec![(
                        PaneFlex(1.),
                        PaneNodeSnapshot::Leaf(LeafSnapshot {
                            is_focused: true,
                            custom_vertical_tabs_title: None,
                            contents: LeafContents::Notebook(NotebookPaneSnapshot::CloudNotebook {
                                notebook_id: None,
                                settings: OpenWarpDriveObjectSettings::default(),
                            }),
                        }),
                    )],
                }),
                left_panel: None,
                right_panel: None,
            },
            TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                root: PaneNodeSnapshot::Branch(BranchSnapshot {
                    direction: SplitDirection::Vertical,
                    children: vec![(
                        PaneFlex(1.),
                        PaneNodeSnapshot::Leaf(LeafSnapshot {
                            is_focused: true,
                            custom_vertical_tabs_title: None,
                            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                                uuid: vec![],
                                cwd: Some("/path/to/dir".into()),
                                is_active: true,
                                is_read_only: false,
                                shell_launch_data: None,
                                input_config: None,
                                llm_model_override: None,
                                active_profile_id: None,
                                conversation_ids_to_restore: vec![],
                                active_conversation_id: None,
                            }),
                        }),
                    )],
                }),
                left_panel: None,
                right_panel: None,
            },
        ],
    );

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert_eq!(template.windows[0].active_tab_index, Some(0))
}

#[test]
fn test_config_with_active_tab_being_filtered() {
    let state = multi_tab_snapshot(
        1,
        vec![
            TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                root: PaneNodeSnapshot::Branch(BranchSnapshot {
                    direction: SplitDirection::Vertical,
                    children: vec![(
                        PaneFlex(1.),
                        PaneNodeSnapshot::Leaf(LeafSnapshot {
                            is_focused: true,
                            custom_vertical_tabs_title: None,
                            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                                uuid: vec![],
                                cwd: Some("/path/to/dir".into()),
                                is_active: true,
                                is_read_only: false,
                                shell_launch_data: None,
                                input_config: None,
                                llm_model_override: None,
                                active_profile_id: None,
                                conversation_ids_to_restore: vec![],
                                active_conversation_id: None,
                            }),
                        }),
                    )],
                }),
                left_panel: None,
                right_panel: None,
            },
            TabSnapshot {
                custom_title: None,
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                root: PaneNodeSnapshot::Branch(BranchSnapshot {
                    direction: SplitDirection::Vertical,
                    children: vec![(
                        PaneFlex(1.),
                        PaneNodeSnapshot::Leaf(LeafSnapshot {
                            is_focused: true,
                            custom_vertical_tabs_title: None,
                            contents: LeafContents::Notebook(NotebookPaneSnapshot::CloudNotebook {
                                notebook_id: None,
                                settings: OpenWarpDriveObjectSettings::default(),
                            }),
                        }),
                    )],
                }),
                left_panel: None,
                right_panel: None,
            },
        ],
    );

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    assert_eq!(template.windows[0].active_tab_index, None)
}

#[test]
fn test_config_from_snapshot_preserves_workspace_groups() {
    let backend_tabs = vec![
        terminal_tab_snapshot("API", "/repo/api"),
        terminal_tab_snapshot("Worker", "/repo/worker"),
    ];
    let ops_tabs = vec![terminal_tab_snapshot("Deploy", "/repo/deploy")];
    let state = AppState {
        windows: vec![WindowSnapshot {
            tabs: ops_tabs.clone(),
            active_tab_index: 0,
            workspace_groups: vec![
                WorkspaceGroupSnapshot {
                    name: "Backend".to_string(),
                    color: AnsiColorIdentifier::Green,
                    tabs: backend_tabs,
                    active_tab_index: 1,
                },
                WorkspaceGroupSnapshot {
                    name: "Ops".to_string(),
                    color: AnsiColorIdentifier::Magenta,
                    tabs: ops_tabs,
                    active_tab_index: 0,
                },
            ],
            active_workspace_group_index: 1,
            bounds: None,
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            fullscreen_state: Default::default(),
            left_panel_width: None,
            right_panel_width: None,
            agent_management_filters: None,
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    let template = LaunchConfig::from_snapshot("Test".into(), &state);
    let window = &template.windows[0];
    assert_eq!(window.active_workspace_group_index, Some(1));
    assert_eq!(window.workspace_groups.len(), 2);
    assert_eq!(window.workspace_groups[0].name, "Backend");
    assert_eq!(
        window.workspace_groups[0].color,
        Some(AnsiColorIdentifier::Green)
    );
    assert_eq!(window.workspace_groups[0].active_tab_index, Some(1));
    assert_eq!(window.workspace_groups[0].tabs.len(), 2);
    assert_eq!(window.workspace_groups[1].name, "Ops");
    assert_eq!(
        window.workspace_groups[1].color,
        Some(AnsiColorIdentifier::Magenta)
    );
    assert_eq!(window.tabs.len(), 1);
    assert_eq!(window.tabs[0].title.as_deref(), Some("Deploy"));

    let yaml = template.to_yaml_string().expect("config should serialize");
    assert!(yaml.contains("workspace_groups:"));
    assert!(yaml.contains("active_workspace_group_index: 1"));
    assert!(yaml.contains("color: green"));
}

#[test]
fn test_config_deserializes_legacy_yaml_without_workspace_groups() {
    let yaml = r#"
name: Legacy
active_window_index: 0
windows:
  - active_tab_index: 0
    tabs:
      - title: Shell
        layout:
          cwd: /tmp
"#;

    let template = LaunchConfig::from_yaml_str(yaml).expect("legacy config should deserialize");
    let window = &template.windows[0];
    assert_eq!(window.active_workspace_group_index, None);
    assert!(window.workspace_groups.is_empty());
    assert_eq!(window.tabs.len(), 1);
}
