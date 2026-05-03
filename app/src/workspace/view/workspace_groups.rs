use crate::appearance::Appearance;
use crate::workspace::{Workspace, WorkspaceAction};
use warp_core::ui::theme::color::internal_colors;
use warpui::elements::{
    Border, ChildView, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment,
    DispatchEventResult, DragAxis, Draggable, Element, EventHandler, Fill, Flex, Hoverable,
    MainAxisAlignment, MainAxisSize, Padding, ParentElement, Radius, SavePosition, Shrinkable,
    Text,
};
use warpui::platform::Cursor;
use warpui::{AppContext, SingletonEntity};

const PANEL_WIDTH: f32 = 220.;
const ROW_RADIUS: f32 = 4.;
const MENU_SLOT_WIDTH: f32 = 22.;

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
                    .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
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

                label_row.add_child(
                    Container::new(
                        Text::new_inline(tab_count.to_string(), appearance.ui_font_family(), 11.)
                            .with_color(theme.sub_text_color(theme.background()).into())
                            .finish(),
                    )
                    .with_padding_left(6.)
                    .with_padding_right(6.)
                    .finish(),
                );

                row.add_child(Shrinkable::new(1., label_row.finish()).finish());

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
}
