use crate::appearance::Appearance;
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
use crate::system::SystemInfo;
use crate::workspace::{Workspace, WorkspaceAction};
use warp_core::ui::theme::color::internal_colors;
use warpui::elements::{
    Border, ChildView, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment,
    DispatchEventResult, DragAxis, Draggable, Element, EventHandler, Expanded, Fill, Flex,
    Hoverable, MainAxisAlignment, MainAxisSize, Padding, ParentElement, Radius, SavePosition,
    Shrinkable, Text,
};
use warpui::platform::Cursor;
use warpui::{AppContext, SingletonEntity};

const PANEL_WIDTH: f32 = 220.;
const ROW_RADIUS: f32 = 4.;
const COUNT_SLOT_WIDTH: f32 = 24.;
const MENU_SLOT_WIDTH: f32 = 22.;

#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
#[derive(Debug, Clone, Copy, PartialEq)]
struct WorkspaceResourceStats {
    cpu_usage: f32,
    gpu_usage: Option<f32>,
    memory_footprint_bytes: u64,
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
        column.add_child(
            Container::new(title)
                .with_padding_left(10.)
                .with_padding_right(8.)
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
            let name = group.name.clone();
            let row_mouse_state = group.mouse_state.clone();
            let draggable_state = group.draggable_state.clone();
            let rename_editor = self.workspace_group_rename_editor.clone();

            let row = Hoverable::new(row_mouse_state, move |state| {
                let mut row = Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center);

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

                let menu_slot = if state.is_hovered() || is_active {
                    EventHandler::new(
                        Container::new(
                            Text::new_inline("...", appearance.ui_font_family(), 11.)
                                .with_color(theme.sub_text_color(theme.background()).into())
                                .finish(),
                        )
                        .with_padding_left(2.)
                        .with_padding_right(2.)
                        .finish(),
                    )
                    .on_left_mouse_down(move |ctx, _, position| {
                        ctx.dispatch_typed_action(
                            WorkspaceAction::ToggleWorkspaceGroupContextMenu { index, position },
                        );
                        DispatchEventResult::StopPropagation
                    })
                    .finish()
                } else {
                    warpui::elements::Empty::new().finish()
                };
                row.add_child(
                    ConstrainedBox::new(menu_slot)
                        .with_width(MENU_SLOT_WIDTH)
                        .finish(),
                );

                let background = if is_active {
                    Fill::Solid(internal_colors::fg_overlay_2(theme).into())
                } else if state.is_hovered() {
                    Fill::Solid(internal_colors::fg_overlay_1(theme).into())
                } else {
                    Fill::None
                };

                Container::new(row.finish())
                    .with_background(background)
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(ROW_RADIUS)))
                    .with_padding_left(10.)
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
        column.add_child(Self::render_workspace_resource_stats(app, appearance));

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
        let label = Self::workspace_resource_stats_label(Self::workspace_resource_stats(app));
        Container::new(
            Text::new_inline(label, appearance.ui_font_family(), 10.)
                .with_color(theme.sub_text_color(theme.background()).into())
                .finish(),
        )
        .with_padding_left(14.)
        .with_padding_right(14.)
        .with_padding_top(4.)
        .with_padding_bottom(2.)
        .finish()
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn workspace_resource_stats(app: &AppContext) -> WorkspaceResourceStats {
        let system_info = SystemInfo::as_ref(app);
        WorkspaceResourceStats {
            cpu_usage: system_info.cpu_usage(),
            gpu_usage: system_info.gpu_usage(),
            memory_footprint_bytes: system_info.memory_footprint().as_u64(),
        }
    }

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    fn workspace_resource_stats_label(stats: WorkspaceResourceStats) -> String {
        let cpu_percent = (stats.cpu_usage.max(0.) * 100.).round() as u32;
        let gpu = stats
            .gpu_usage
            .map(|usage| format!("{}%", (usage.max(0.) * 100.).round() as u32))
            .unwrap_or_else(|| "n/a".to_string());
        format!(
            "CPU {cpu_percent}% | GPU {gpu} | MEM {}",
            Self::format_resource_bytes(stats.memory_footprint_bytes)
        )
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
}

#[cfg(test)]
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
mod tests {
    use super::*;

    #[test]
    fn workspace_resource_stats_label_formats_available_gpu_usage() {
        assert_eq!(
            Workspace::workspace_resource_stats_label(WorkspaceResourceStats {
                cpu_usage: 0.123,
                gpu_usage: Some(0.084),
                memory_footprint_bytes: 1536 * 1024 * 1024,
            }),
            "CPU 12% | GPU 8% | MEM 1.5 GB"
        );
    }

    #[test]
    fn workspace_resource_stats_label_formats_unavailable_gpu() {
        assert_eq!(
            Workspace::workspace_resource_stats_label(WorkspaceResourceStats {
                cpu_usage: 0.025,
                gpu_usage: None,
                memory_footprint_bytes: 512 * 1024 * 1024,
            }),
            "CPU 3% | GPU n/a | MEM 512 MB"
        );
    }

    #[test]
    fn workspace_resource_stats_label_allows_multicore_cpu_usage() {
        assert_eq!(
            Workspace::workspace_resource_stats_label(WorkspaceResourceStats {
                cpu_usage: 1.37,
                gpu_usage: Some(1.23),
                memory_footprint_bytes: 12 * 1024 * 1024 * 1024,
            }),
            "CPU 137% | GPU 123% | MEM 12 GB"
        );
    }
}
