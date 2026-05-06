use crate::appearance::Appearance;
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
use crate::system::{ResourceUsageSample, SystemInfo};
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
use crate::terminal::cli_agent_sessions::codex_rate_limits::{
    estimate_codex_rate_limit_projection, CodexRateLimitProjection, CodexRateLimitUsage,
    CodexRateLimitUsageModel, CodexRateLimitWindowKind, CodexRateLimitWindowUsage,
};
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
use crate::workspace::tab_settings::TabSettings;
use crate::workspace::{Workspace, WorkspaceAction};
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
use chrono::{DateTime, Local, Utc};
use warp_core::ui::color::coloru_with_opacity;
use warp_core::ui::theme::color::internal_colors;
use warp_core::ui::Icon;
use warpui::elements::{
    Border, ChildView, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment,
    DispatchEventResult, DragAxis, Draggable, Element, EventHandler, Expanded, Fill, Flex,
    Hoverable, MainAxisAlignment, MainAxisSize, Padding, ParentElement, Radius, Rect, SavePosition,
    Shrinkable, Stack, Text,
};
use warpui::platform::Cursor;
use warpui::{color::ColorU, AppContext, SingletonEntity};

const PANEL_WIDTH: f32 = 220.;
const ROW_RADIUS: f32 = 4.;
const ROW_LEFT_PADDING: f32 = 10.;
const COLOR_SLOT_WIDTH: f32 = 16.;
const COLOR_LABEL_GAP_WIDTH: f32 = 10.;
const WORKSPACE_COLOR_INDICATOR_SIZE: f32 = 14.;
const WORKSPACE_COLOR_DOT_SIZE: f32 = 6.;
const COUNT_SLOT_WIDTH: f32 = 24.;
const CLOSE_SLOT_WIDTH: f32 = 22.;
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
const RESOURCE_GRAPH_BAR_COUNT: usize = 72;
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
const RESOURCE_GRAPH_HEIGHT: f32 = 18.;
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
const RESOURCE_GRAPH_BAR_WIDTH: f32 = 1.5;
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
const RESOURCE_MONITOR_HORIZONTAL_PADDING: f32 = 14.;
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
const RESOURCE_MONITOR_CONTENT_WIDTH: f32 =
    PANEL_WIDTH - (2. * RESOURCE_MONITOR_HORIZONTAL_PADDING);
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
const RESOURCE_METRIC_FONT_SIZE: f32 = 10.5;
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
const RESOURCE_TIME_FONT_SIZE: f32 = 9.;
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
const CODEX_LIMIT_BAR_HEIGHT: f32 = 8.;
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
#[derive(Debug, Clone, PartialEq)]
struct WorkspaceResourceStats {
    cpu_usage: f32,
    gpu_usage: Option<f32>,
    memory_footprint_bytes: u64,
    memory_usage: Option<f32>,
    history: Vec<ResourceUsageSample>,
}

impl Workspace {
    pub(super) fn render_workspace_groups_panel(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let mut column = Flex::column()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch);

        let title = Text::new_inline("Workspaces", appearance.ui_font_family(), 11.)
            .with_color(theme.sub_text_color(theme.background()).into())
            .finish();
        let icon_color = theme.sub_text_color(theme.background());
        let export_button = EventHandler::new(
            ConstrainedBox::new(Icon::Download.to_warpui_icon(icon_color).finish())
                .with_width(14.)
                .with_height(14.)
                .finish(),
        )
        .on_left_mouse_down(|ctx, _, _| {
            ctx.dispatch_typed_action(WorkspaceAction::ExportWorkspaces);
            DispatchEventResult::StopPropagation
        })
        .finish();
        let import_button = EventHandler::new(
            ConstrainedBox::new(Icon::Import.to_warpui_icon(icon_color).finish())
                .with_width(14.)
                .with_height(14.)
                .finish(),
        )
        .on_left_mouse_down(|ctx, _, _| {
            ctx.dispatch_typed_action(WorkspaceAction::ImportWorkspaces);
            DispatchEventResult::StopPropagation
        })
        .finish();
        let mut title_row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        title_row.add_child(Expanded::new(1., title).finish());
        title_row.add_child(
            Container::new(export_button)
                .with_padding(Padding::uniform(4.))
                .finish(),
        );
        title_row.add_child(
            Container::new(import_button)
                .with_padding(Padding::uniform(4.))
                .finish(),
        );
        column.add_child(
            Container::new(title_row.finish())
                .with_padding_left(10.)
                .with_padding_right(4.)
                .with_padding_top(10.)
                .with_padding_bottom(6.)
                .finish(),
        );

        for (index, group) in self.workspace_groups.iter().enumerate() {
            let is_active = index == self.active_workspace_group_index;
            let is_renaming =
                self.current_workspace_state.workspace_group_being_renamed() == Some(index);
            let tab_count = if is_active {
                self.tabs.len()
            } else {
                group.tabs.len()
            };
            let has_unread_attention = self.workspace_group_has_unread_notifications(index, app);
            let flash_attention =
                has_unread_attention && self.workspace_group_notification_flash_on;
            let name = group.name.clone();
            let color = group.color;
            let row_mouse_state = group.mouse_state.clone();
            let draggable_state = group.draggable_state.clone();
            let rename_editor = self.workspace_group_rename_editor.clone();
            let row = Hoverable::new(row_mouse_state, move |state| {
                let mut row = Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center);

                let workspace_color: ColorU =
                    color.to_ansi_color(&theme.terminal_colors().normal).into();
                let color_dot = ConstrainedBox::new(
                    Icon::Ellipse
                        .to_warpui_icon(workspace_color.into())
                        .finish(),
                )
                .with_width(WORKSPACE_COLOR_DOT_SIZE)
                .with_height(WORKSPACE_COLOR_DOT_SIZE)
                .finish();
                let color_indicator = ConstrainedBox::new(
                    Container::new(color_dot)
                        .with_background_color(coloru_with_opacity(workspace_color, 18))
                        .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
                        .with_padding(Padding::uniform(4.))
                        .finish(),
                )
                .with_width(WORKSPACE_COLOR_INDICATOR_SIZE)
                .with_height(WORKSPACE_COLOR_INDICATOR_SIZE)
                .finish();
                let color_button = EventHandler::new(color_indicator)
                    .on_left_mouse_down(move |ctx, _, position| {
                        ctx.dispatch_typed_action(WorkspaceAction::ToggleWorkspaceGroupColorMenu {
                            index,
                            position,
                        });
                        DispatchEventResult::StopPropagation
                    })
                    .finish();
                row.add_child(
                    ConstrainedBox::new(color_button)
                        .with_width(COLOR_SLOT_WIDTH)
                        .finish(),
                );
                row.add_child(
                    ConstrainedBox::new(warpui::elements::Empty::new().finish())
                        .with_width(COLOR_LABEL_GAP_WIDTH)
                        .finish(),
                );

                let mut label_row =
                    Flex::row().with_cross_axis_alignment(CrossAxisAlignment::Center);
                if is_renaming {
                    label_row.add_child(
                        Shrinkable::new(1., ChildView::new(&rename_editor).finish()).finish(),
                    );
                } else {
                    label_row.add_child(
                        Shrinkable::new(
                            1.,
                            Text::new_inline(name.clone(), appearance.ui_font_family(), 12.)
                                .with_color(theme.main_text_color(theme.background()).into())
                                .finish(),
                        )
                        .finish(),
                    );
                }

                row.add_child(Expanded::new(1., label_row.finish()).finish());

                let mut count_row = Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_main_axis_alignment(MainAxisAlignment::End)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center);
                count_row.add_child(
                    Text::new_inline(tab_count.to_string(), appearance.ui_font_family(), 11.)
                        .with_color(theme.sub_text_color(theme.background()).into())
                        .finish(),
                );
                row.add_child(
                    ConstrainedBox::new(count_row.finish())
                        .with_width(COUNT_SLOT_WIDTH)
                        .finish(),
                );

                let close_slot = EventHandler::new(
                    ConstrainedBox::new(
                        Icon::X
                            .to_warpui_icon(theme.sub_text_color(theme.background()))
                            .finish(),
                    )
                    .with_width(11.)
                    .with_height(11.)
                    .finish(),
                )
                .on_left_mouse_down(move |ctx, _, _position| {
                    ctx.dispatch_typed_action(WorkspaceAction::CloseWorkspaceGroup(index));
                    DispatchEventResult::StopPropagation
                })
                .finish();
                row.add_child(
                    ConstrainedBox::new(close_slot)
                        .with_width(CLOSE_SLOT_WIDTH)
                        .finish(),
                );

                let background = if flash_attention {
                    Fill::Solid(theme.accent().with_opacity(25).into_solid().into())
                } else if is_active {
                    Fill::Solid(internal_colors::fg_overlay_2(theme).into())
                } else if state.is_hovered() {
                    Fill::Solid(internal_colors::fg_overlay_1(theme).into())
                } else {
                    Fill::None
                };

                Container::new(row.finish())
                    .with_background(background)
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(ROW_RADIUS)))
                    .with_padding_left(ROW_LEFT_PADDING)
                    .with_padding_right(8.)
                    .with_padding_top(6.)
                    .with_padding_bottom(6.)
                    .finish()
            })
            .on_click(move |ctx, _, _| {
                if !is_renaming {
                    ctx.dispatch_typed_action(WorkspaceAction::ActivateWorkspaceGroup(index));
                }
            })
            .on_double_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(WorkspaceAction::RenameWorkspaceGroup(index));
            })
            .on_right_click(move |ctx, _, position| {
                ctx.dispatch_typed_action(WorkspaceAction::ToggleWorkspaceGroupContextMenu {
                    index,
                    position,
                });
            })
            .with_cursor(Cursor::PointingHand)
            .finish();

            let row = Draggable::new(draggable_state, row)
                .with_drag_axis(DragAxis::VerticalOnly)
                .on_drag_start(move |ctx, _, _| {
                    ctx.dispatch_typed_action(WorkspaceAction::StartWorkspaceGroupDrag(index));
                })
                .on_drag(move |ctx, _, position, _| {
                    ctx.dispatch_typed_action(WorkspaceAction::DragWorkspaceGroup {
                        index,
                        position,
                    });
                })
                .on_drop(|ctx, _, _, _| {
                    ctx.dispatch_typed_action(WorkspaceAction::DropWorkspaceGroup);
                })
                .finish();

            column.add_child(
                SavePosition::new(
                    Container::new(row)
                        .with_padding_left(6.)
                        .with_padding_right(6.)
                        .with_padding_bottom(2.)
                        .finish(),
                    &Workspace::workspace_group_position_id(index),
                )
                .finish(),
            );
        }

        column.add_child(Shrinkable::new(1., warpui::elements::Empty::new().finish()).finish());

        #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
        if Self::show_workspace_resource_monitor(app) {
            column.add_child(Self::render_workspace_resource_stats(app, appearance));
        }

        let add_row = EventHandler::new(
            Container::new(
                Text::new_inline("+ New workspace", appearance.ui_font_family(), 12.)
                    .with_color(theme.main_text_color(theme.background()).into())
                    .finish(),
            )
            .with_padding(Padding::uniform(8.))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(ROW_RADIUS)))
            .finish(),
        )
        .on_left_mouse_down(|ctx, _, _| {
            ctx.dispatch_typed_action(WorkspaceAction::AddWorkspaceGroup);
            warpui::elements::DispatchEventResult::StopPropagation
        })
        .finish();

        column.add_child(
            Container::new(add_row)
                .with_padding_left(6.)
                .with_padding_right(6.)
                .with_padding_bottom(8.)
                .finish(),
        );

        ConstrainedBox::new(
            Container::new(column.finish())
                .with_background(theme.surface_1())
                .with_border(Border::right(1.).with_border_fill(theme.outline()))
                .finish(),
        )
        .with_width(PANEL_WIDTH)
        .finish()
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn render_workspace_resource_stats(
        app: &AppContext,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let stats = Self::workspace_resource_stats(app);
        let codex_usage = CodexRateLimitUsageModel::as_ref(app).usage();
        let mut column = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);

        column.add_child(Self::render_codex_rate_limit_stats(
            &codex_usage,
            appearance,
        ));
        column.add_child(Self::render_resource_metric_row(
            "CPU",
            Self::workspace_resource_percent_label(Some(stats.cpu_usage)),
            stats.history.iter().map(|sample| Some(sample.cpu_usage)),
            appearance,
        ));
        column.add_child(Self::render_resource_metric_row(
            "GPU",
            Self::workspace_resource_percent_label(stats.gpu_usage),
            stats.history.iter().map(|sample| sample.gpu_usage),
            appearance,
        ));
        column.add_child(Self::render_resource_metric_row(
            "Memory",
            Self::format_resource_bytes(stats.memory_footprint_bytes),
            stats.history.iter().map(|sample| sample.memory_usage),
            appearance,
        ));

        let mut time_labels = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        time_labels.add_child(
            Text::new_inline(
                "30m Ago",
                appearance.ui_font_family(),
                RESOURCE_TIME_FONT_SIZE,
            )
            .with_color(theme.sub_text_color(theme.background()).into())
            .finish(),
        );
        time_labels.add_child(Expanded::new(1., warpui::elements::Empty::new().finish()).finish());
        time_labels.add_child(
            Text::new_inline("Now", appearance.ui_font_family(), RESOURCE_TIME_FONT_SIZE)
                .with_color(theme.sub_text_color(theme.background()).into())
                .finish(),
        );
        column.add_child(
            ConstrainedBox::new(time_labels.finish())
                .with_width(RESOURCE_MONITOR_CONTENT_WIDTH)
                .finish(),
        );

        Container::new(column.finish())
            .with_padding_left(RESOURCE_MONITOR_HORIZONTAL_PADDING)
            .with_padding_right(RESOURCE_MONITOR_HORIZONTAL_PADDING)
            .with_padding_top(6.)
            .with_padding_bottom(6.)
            .finish()
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn render_codex_rate_limit_stats(
        usage: &CodexRateLimitUsage,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let mut column = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);
        let current = usage.current.as_ref();

        if let Some(primary) = current.and_then(|sample| sample.primary.as_ref()) {
            column.add_child(Self::render_codex_rate_limit_card(primary, appearance));
        }

        if let Some(secondary) = current.and_then(|sample| sample.secondary.as_ref()) {
            column.add_child(Self::render_codex_rate_limit_card(secondary, appearance));
        }

        let status_label = Self::codex_rate_limit_status_label(usage, Utc::now());
        column.add_child(Self::render_codex_rate_limit_status(
            status_label,
            appearance,
        ));

        Container::new(column.finish())
            .with_padding_bottom(2.)
            .finish()
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn render_codex_rate_limit_card(
        window: &CodexRateLimitWindowUsage,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let title_color = theme.sub_text_color(theme.background()).into();
        let value_color = theme.main_text_color(theme.background()).into();
        let reset_color = theme.sub_text_color(theme.background()).into();
        let mut column = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);

        column.add_child(
            Text::new_inline(
                Self::codex_rate_limit_window_title(window),
                appearance.ui_font_family(),
                RESOURCE_METRIC_FONT_SIZE,
            )
            .with_color(title_color)
            .finish(),
        );
        column.add_child(
            Container::new(
                Text::new_inline(
                    Self::codex_rate_limit_remaining_label(window.remaining_percent),
                    appearance.ui_font_family(),
                    RESOURCE_METRIC_FONT_SIZE,
                )
                .with_color(value_color)
                .finish(),
            )
            .with_padding_top(2.)
            .finish(),
        );
        column.add_child(
            Container::new(Self::render_codex_rate_limit_bar(
                Self::codex_rate_limit_progress_fraction(window.remaining_percent),
                appearance,
            ))
            .with_padding_top(6.)
            .finish(),
        );
        column.add_child(
            Container::new(
                Text::new_inline(
                    Self::codex_rate_limit_reset_label(window),
                    appearance.ui_font_family(),
                    RESOURCE_TIME_FONT_SIZE,
                )
                .with_color(reset_color)
                .finish(),
            )
            .with_padding_top(6.)
            .finish(),
        );

        Container::new(
            ConstrainedBox::new(column.finish())
                .with_width(RESOURCE_MONITOR_CONTENT_WIDTH)
                .finish(),
        )
        .with_padding_bottom(8.)
        .finish()
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn render_codex_rate_limit_bar(
        progress_fraction: f32,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let track_color = internal_colors::fg_overlay_2(theme).into();
        let fill_color = theme.terminal_colors().normal.green.into();
        let fill_width = (RESOURCE_MONITOR_CONTENT_WIDTH * progress_fraction)
            .clamp(0., RESOURCE_MONITOR_CONTENT_WIDTH);
        let radius = CornerRadius::with_all(Radius::Percentage(50.));

        let track = Rect::new()
            .with_background_color(track_color)
            .with_corner_radius(radius)
            .finish();
        let fill = Rect::new()
            .with_background_color(fill_color)
            .with_corner_radius(radius)
            .finish();

        let mut stack = Stack::new();
        stack.add_child(
            ConstrainedBox::new(track)
                .with_width(RESOURCE_MONITOR_CONTENT_WIDTH)
                .with_height(CODEX_LIMIT_BAR_HEIGHT)
                .finish(),
        );
        stack.add_child(
            ConstrainedBox::new(fill)
                .with_width(fill_width)
                .with_height(CODEX_LIMIT_BAR_HEIGHT)
                .finish(),
        );

        ConstrainedBox::new(stack.finish())
            .with_width(RESOURCE_MONITOR_CONTENT_WIDTH)
            .with_height(CODEX_LIMIT_BAR_HEIGHT)
            .finish()
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn render_codex_rate_limit_status(label: String, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let mut status_row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        status_row.add_child(
            Text::new_inline(label, appearance.ui_font_family(), RESOURCE_TIME_FONT_SIZE)
                .with_color(theme.sub_text_color(theme.background()).into())
                .finish(),
        );

        ConstrainedBox::new(status_row.finish())
            .with_width(RESOURCE_MONITOR_CONTENT_WIDTH)
            .finish()
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn render_resource_metric_row(
        name: impl Into<String>,
        current_value: String,
        samples: impl Iterator<Item = Option<f32>>,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let text_color = theme.sub_text_color(theme.background()).into();
        let graph_color = theme.sub_text_color(theme.background()).into();
        let guide_color = internal_colors::fg_overlay_2(theme).into();

        let mut label_row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        label_row.add_child(
            Text::new_inline(
                name.into(),
                appearance.ui_font_family(),
                RESOURCE_METRIC_FONT_SIZE,
            )
            .with_color(text_color)
            .finish(),
        );
        label_row.add_child(Expanded::new(1., warpui::elements::Empty::new().finish()).finish());
        label_row.add_child(
            Text::new_inline(
                current_value,
                appearance.ui_font_family(),
                RESOURCE_METRIC_FONT_SIZE,
            )
            .with_color(text_color)
            .finish(),
        );

        let mut metric = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);
        metric.add_child(
            ConstrainedBox::new(label_row.finish())
                .with_width(RESOURCE_MONITOR_CONTENT_WIDTH)
                .finish(),
        );
        metric.add_child(Self::render_resource_graph(
            Self::downsample_resource_values(samples),
            graph_color,
            guide_color,
        ));

        Container::new(metric.finish())
            .with_padding_bottom(4.)
            .finish()
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn render_resource_graph(
        values: Vec<f32>,
        color: ColorU,
        guide_color: ColorU,
    ) -> Box<dyn Element> {
        let mut stack = Stack::new();
        stack.add_child(Self::render_resource_level_guides(guide_color));
        stack.add_child(Self::render_resource_bars(values, color));

        ConstrainedBox::new(stack.finish())
            .with_width(RESOURCE_MONITOR_CONTENT_WIDTH)
            .with_height(RESOURCE_GRAPH_HEIGHT)
            .finish()
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn render_resource_level_guides(color: ColorU) -> Box<dyn Element> {
        let mut guides = Flex::column()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch);

        for _ in 0..=2 {
            guides.add_child(
                ConstrainedBox::new(Rect::new().with_background_color(color).finish())
                    .with_width(RESOURCE_MONITOR_CONTENT_WIDTH)
                    .with_height(1.)
                    .finish(),
            );
        }

        ConstrainedBox::new(guides.finish())
            .with_width(RESOURCE_MONITOR_CONTENT_WIDTH)
            .with_height(RESOURCE_GRAPH_HEIGHT)
            .finish()
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn render_resource_bars(values: Vec<f32>, color: ColorU) -> Box<dyn Element> {
        let mut graph = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::End);

        for value in values {
            let height = (value.clamp(0., 1.) * RESOURCE_GRAPH_HEIGHT).max(1.);
            graph.add_child(
                ConstrainedBox::new(Rect::new().with_background_color(color).finish())
                    .with_width(RESOURCE_GRAPH_BAR_WIDTH)
                    .with_height(height)
                    .finish(),
            );
        }

        ConstrainedBox::new(graph.finish())
            .with_width(RESOURCE_MONITOR_CONTENT_WIDTH)
            .with_height(RESOURCE_GRAPH_HEIGHT)
            .finish()
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn codex_rate_limit_window_title(window: &CodexRateLimitWindowUsage) -> String {
        match (window.kind, window.window_duration_mins) {
            (CodexRateLimitWindowKind::Primary, Some(300)) => "5 hour usage limit".to_string(),
            (CodexRateLimitWindowKind::Secondary, Some(10080)) => "Weekly usage limit".to_string(),
            (_, Some(minutes)) => {
                format!(
                    "{} usage limit",
                    Self::format_codex_rate_limit_window_duration(minutes)
                )
            }
            (CodexRateLimitWindowKind::Primary, None) => "Codex usage limit".to_string(),
            (CodexRateLimitWindowKind::Secondary, None) => "Codex weekly usage limit".to_string(),
        }
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn codex_rate_limit_remaining_label(remaining_percent: f32) -> String {
        format!(
            "{}% remaining",
            remaining_percent.clamp(0., 100.).round() as u32
        )
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn codex_rate_limit_progress_fraction(remaining_percent: f32) -> f32 {
        (remaining_percent / 100.).clamp(0., 1.)
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn codex_rate_limit_reset_label(window: &CodexRateLimitWindowUsage) -> String {
        match window.resets_at {
            Some(resets_at) => {
                let local_reset = resets_at.with_timezone(&Local);
                let is_long_window = window
                    .window_duration_mins
                    .is_some_and(|minutes| minutes >= 24 * 60);
                if is_long_window {
                    format!("Resets {}", local_reset.format("%b %d %H:%M"))
                } else {
                    format!("Resets {}", local_reset.format("%H:%M"))
                }
            }
            None => "Reset unavailable".to_string(),
        }
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn codex_rate_limit_status_label(usage: &CodexRateLimitUsage, now: DateTime<Utc>) -> String {
        if usage.is_stale {
            return "Stale".to_string();
        }
        if usage.last_error.is_some() && usage.current.is_none() {
            return "Unavailable".to_string();
        }
        match estimate_codex_rate_limit_projection(usage, now) {
            CodexRateLimitProjection::EmptyNow => "Limit reached".to_string(),
            CodexRateLimitProjection::EmptyAt(empty_at) => {
                format!(
                    "Empty in {}",
                    Self::format_codex_rate_limit_duration(empty_at - now)
                )
            }
            CodexRateLimitProjection::ResetsAt(resets_at) => {
                format!("Reset {}", resets_at.with_timezone(&Local).format("%H:%M"))
            }
            CodexRateLimitProjection::Stable if usage.current.is_some() => "Stable".to_string(),
            CodexRateLimitProjection::Stable => "Loading".to_string(),
            CodexRateLimitProjection::Unknown => "Waiting for data".to_string(),
        }
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn format_codex_rate_limit_duration(duration: chrono::Duration) -> String {
        let total_minutes = ((duration.num_seconds().max(0) + 59) / 60).max(1);
        if total_minutes < 60 {
            return format!("{total_minutes}m");
        }

        let hours = total_minutes / 60;
        let minutes = total_minutes % 60;
        if minutes == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {minutes}m")
        }
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn format_codex_rate_limit_window_duration(minutes: u32) -> String {
        if minutes % 10080 == 0 {
            let weeks = minutes / 10080;
            if weeks == 1 {
                "Weekly".to_string()
            } else {
                format!("{weeks} week")
            }
        } else if minutes % 60 == 0 {
            let hours = minutes / 60;
            if hours == 1 {
                "1 hour".to_string()
            } else {
                format!("{hours} hour")
            }
        } else {
            format!("{minutes} minute")
        }
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn workspace_resource_stats(app: &AppContext) -> WorkspaceResourceStats {
        let system_info = SystemInfo::as_ref(app);
        let current = system_info.current_resource_usage_sample();
        WorkspaceResourceStats {
            cpu_usage: current.cpu_usage,
            gpu_usage: current.gpu_usage,
            memory_footprint_bytes: current.memory_footprint_bytes,
            memory_usage: current.memory_usage,
            history: system_info.resource_usage_history().collect(),
        }
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn show_workspace_resource_monitor(app: &AppContext) -> bool {
        *TabSettings::as_ref(app).show_workspace_resource_monitor
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn workspace_resource_percent_label(value: Option<f32>) -> String {
        value
            .map(|usage| format!("{}%", (usage.max(0.) * 100.).round() as u32))
            .unwrap_or_else(|| "n/a".to_string())
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn format_resource_bytes(bytes: u64) -> String {
        const KB: f64 = 1024.;
        const MB: f64 = KB * 1024.;
        const GB: f64 = MB * 1024.;

        let bytes = bytes as f64;
        if bytes >= 10. * GB {
            format!("{:.0} GB", bytes / GB)
        } else if bytes >= GB {
            format!("{:.1} GB", bytes / GB)
        } else if bytes >= 10. * MB {
            format!("{:.0} MB", bytes / MB)
        } else if bytes >= MB {
            format!("{:.1} MB", bytes / MB)
        } else if bytes >= KB {
            format!("{:.0} KB", bytes / KB)
        } else {
            format!("{bytes:.0} B")
        }
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn downsample_resource_values(samples: impl Iterator<Item = Option<f32>>) -> Vec<f32> {
        let values: Vec<f32> = samples.map(|sample| sample.unwrap_or(0.).max(0.)).collect();
        if values.is_empty() {
            return vec![0.];
        }

        if values.len() <= RESOURCE_GRAPH_BAR_COUNT {
            return values;
        }

        (0..RESOURCE_GRAPH_BAR_COUNT)
            .map(|index| {
                let start = index * values.len() / RESOURCE_GRAPH_BAR_COUNT;
                let end = ((index + 1) * values.len() / RESOURCE_GRAPH_BAR_COUNT).max(start + 1);
                values[start..end].iter().copied().fold(0., f32::max)
            })
            .collect()
    }
}

#[cfg(test)]
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
mod tests {
    use super::*;
    use crate::terminal::cli_agent_sessions::codex_rate_limits::CodexRateLimitUsageSample;

    fn codex_window(
        kind: CodexRateLimitWindowKind,
        window_duration_mins: Option<u32>,
        remaining_percent: f32,
        resets_at: Option<DateTime<Utc>>,
    ) -> CodexRateLimitWindowUsage {
        CodexRateLimitWindowUsage {
            kind,
            used_percent: 100. - remaining_percent,
            remaining_percent,
            window_duration_mins,
            resets_at,
        }
    }

    fn codex_usage_sample(
        fetched_at: DateTime<Utc>,
        primary_remaining_percent: Option<f32>,
        secondary_remaining_percent: Option<f32>,
        resets_at: Option<DateTime<Utc>>,
    ) -> CodexRateLimitUsageSample {
        CodexRateLimitUsageSample {
            fetched_at,
            primary: primary_remaining_percent.map(|remaining_percent| {
                codex_window(
                    CodexRateLimitWindowKind::Primary,
                    Some(300),
                    remaining_percent,
                    resets_at,
                )
            }),
            secondary: secondary_remaining_percent.map(|remaining_percent| {
                codex_window(
                    CodexRateLimitWindowKind::Secondary,
                    Some(10080),
                    remaining_percent,
                    resets_at,
                )
            }),
            plan_type: Some("pro".to_string()),
            rate_limit_reached_type: None,
        }
    }

    #[test]
    fn workspace_resource_percent_label_formats_available_value() {
        assert_eq!(
            Workspace::workspace_resource_percent_label(Some(0.084)),
            "8%"
        );
    }

    #[test]
    fn workspace_resource_percent_label_formats_unavailable_value() {
        assert_eq!(Workspace::workspace_resource_percent_label(None), "n/a");
    }

    #[test]
    fn workspace_resource_percent_label_allows_multicore_cpu_usage() {
        assert_eq!(
            Workspace::workspace_resource_percent_label(Some(1.37)),
            "137%"
        );
    }

    #[test]
    fn downsample_resource_values_preserves_short_history() {
        assert_eq!(
            Workspace::downsample_resource_values([Some(0.1), None, Some(0.3)].into_iter()),
            vec![0.1, 0., 0.3]
        );
    }

    #[test]
    fn downsample_resource_values_limits_long_history_with_bucket_max() {
        let values = (0..RESOURCE_GRAPH_BAR_COUNT * 2)
            .map(|index| Some(index as f32))
            .collect::<Vec<_>>();
        let downsampled = Workspace::downsample_resource_values(values.into_iter());

        assert_eq!(downsampled.len(), RESOURCE_GRAPH_BAR_COUNT);
        assert_eq!(downsampled[0], 1.);
        assert_eq!(
            downsampled[RESOURCE_GRAPH_BAR_COUNT - 1],
            (RESOURCE_GRAPH_BAR_COUNT * 2 - 1) as f32
        );
    }

    #[test]
    fn codex_rate_limit_remaining_label_formats_remaining_percentage() {
        assert_eq!(
            Workspace::codex_rate_limit_remaining_label(79.4),
            "79% remaining"
        );
    }

    #[test]
    fn codex_rate_limit_window_title_names_five_hour_limit() {
        assert_eq!(
            Workspace::codex_rate_limit_window_title(&codex_window(
                CodexRateLimitWindowKind::Primary,
                Some(300),
                99.,
                None,
            )),
            "5 hour usage limit"
        );
    }

    #[test]
    fn codex_rate_limit_window_title_names_weekly_limit() {
        assert_eq!(
            Workspace::codex_rate_limit_window_title(&codex_window(
                CodexRateLimitWindowKind::Secondary,
                Some(10080),
                99.,
                None,
            )),
            "Weekly usage limit"
        );
    }

    #[test]
    fn codex_rate_limit_reset_label_uses_local_time() {
        let reset = DateTime::<Utc>::from_timestamp(1777880803, 0).unwrap();
        let window = codex_window(
            CodexRateLimitWindowKind::Primary,
            Some(300),
            99.,
            Some(reset),
        );

        assert_eq!(
            Workspace::codex_rate_limit_reset_label(&window),
            format!("Resets {}", reset.with_timezone(&Local).format("%H:%M"))
        );
    }

    #[test]
    fn codex_rate_limit_reset_label_includes_date_for_weekly_limit() {
        let reset = DateTime::<Utc>::from_timestamp(1777880803, 0).unwrap();
        let window = codex_window(
            CodexRateLimitWindowKind::Secondary,
            Some(10080),
            99.,
            Some(reset),
        );

        assert_eq!(
            Workspace::codex_rate_limit_reset_label(&window),
            format!(
                "Resets {}",
                reset.with_timezone(&Local).format("%b %d %H:%M")
            )
        );
    }

    #[test]
    fn codex_rate_limit_reset_label_handles_missing_timestamp() {
        let window = codex_window(CodexRateLimitWindowKind::Primary, Some(300), 99., None);

        assert_eq!(
            Workspace::codex_rate_limit_reset_label(&window),
            "Reset unavailable"
        );
    }

    #[test]
    fn codex_rate_limit_progress_fraction_clamps_to_progress_range() {
        assert_eq!(Workspace::codex_rate_limit_progress_fraction(-10.), 0.);
        assert_eq!(Workspace::codex_rate_limit_progress_fraction(42.), 0.42);
        assert_eq!(Workspace::codex_rate_limit_progress_fraction(120.), 1.);
    }

    #[test]
    fn codex_rate_limit_status_label_uses_compact_loading_states() {
        assert_eq!(
            Workspace::codex_rate_limit_status_label(
                &CodexRateLimitUsage::default(),
                DateTime::UNIX_EPOCH,
            ),
            "Loading"
        );
    }

    #[test]
    fn codex_rate_limit_status_label_estimates_empty_time() {
        let base = DateTime::UNIX_EPOCH;
        let reset = base + chrono::Duration::hours(3);
        let history = vec![
            codex_usage_sample(base, Some(30.), None, Some(reset)),
            codex_usage_sample(
                base + chrono::Duration::minutes(10),
                Some(20.),
                None,
                Some(reset),
            ),
            codex_usage_sample(
                base + chrono::Duration::minutes(20),
                Some(10.),
                None,
                Some(reset),
            ),
        ];
        let usage = CodexRateLimitUsage {
            current: history.last().cloned(),
            history,
            last_error: None,
            is_stale: false,
        };

        assert_eq!(
            Workspace::codex_rate_limit_status_label(&usage, base + chrono::Duration::minutes(20),),
            "Empty in 10m"
        );
    }

    #[test]
    fn codex_rate_limit_status_label_prefers_reset_when_it_happens_first() {
        let base = DateTime::UNIX_EPOCH;
        let reset = base + chrono::Duration::minutes(25);
        let history = vec![
            codex_usage_sample(base, Some(30.), None, Some(reset)),
            codex_usage_sample(
                base + chrono::Duration::minutes(10),
                Some(25.),
                None,
                Some(reset),
            ),
            codex_usage_sample(
                base + chrono::Duration::minutes(20),
                Some(20.),
                None,
                Some(reset),
            ),
        ];
        let usage = CodexRateLimitUsage {
            current: history.last().cloned(),
            history,
            last_error: None,
            is_stale: false,
        };

        assert_eq!(
            Workspace::codex_rate_limit_status_label(&usage, base + chrono::Duration::minutes(20),),
            format!("Reset {}", reset.with_timezone(&Local).format("%H:%M"))
        );
    }
}
