use warp_core::ui::appearance::Appearance;
use warpui::{
    elements::{
        Align, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, DispatchEventResult,
        Element, EventHandler, Flex, MainAxisAlignment, MainAxisSize, ParentElement as _, Radius,
        Rect, Text,
    },
    AppContext, Entity, ModelHandle, SingletonEntity as _, TypedActionView, View, ViewContext,
};

use crate::{
    pane_group::{
        focus_state::PaneFocusHandle, pane::view, BackingView, PaneConfiguration, PaneEvent,
        TerminalLayoutAgentMode, TerminalLayoutPreset,
    },
    workspace::WorkspaceAction,
};

#[derive(Debug, Clone, Copy)]
pub enum WorkspaceLayoutChooserAction {
    SetAgentMode(TerminalLayoutAgentMode),
}

pub struct WorkspaceLayoutChooserView {
    pane_configuration: ModelHandle<PaneConfiguration>,
    selected_agent_mode: TerminalLayoutAgentMode,
    focus_handle: Option<PaneFocusHandle>,
}

impl WorkspaceLayoutChooserView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        Self {
            pane_configuration: ctx.add_model(|_| PaneConfiguration::new("Choose layout")),
            selected_agent_mode: TerminalLayoutAgentMode::None,
            focus_handle: None,
        }
    }

    pub fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    pub fn selected_agent_mode(&self) -> TerminalLayoutAgentMode {
        self.selected_agent_mode
    }

    pub fn set_focus_handle(
        &mut self,
        focus_handle: PaneFocusHandle,
        _ctx: &mut ViewContext<Self>,
    ) {
        self.focus_handle = Some(focus_handle);
    }

    fn render_preview_cell(app: &AppContext, width: f32, height: f32) -> Box<dyn Element> {
        let theme = Appearance::as_ref(app).theme();
        ConstrainedBox::new(
            Rect::new()
                .with_background_color(theme.foreground().with_opacity(18).into())
                .finish(),
        )
        .with_width(width)
        .with_height(height)
        .finish()
    }

    fn render_preview(&self, preset: TerminalLayoutPreset, app: &AppContext) -> Box<dyn Element> {
        let mut outer = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        match preset {
            TerminalLayoutPreset::Single => {
                outer.add_child(Self::render_preview_cell(app, 92., 54.));
            }
            TerminalLayoutPreset::TwoColumns => {
                let mut row = Flex::row().with_main_axis_size(MainAxisSize::Min);
                row.add_child(Self::render_preview_cell(app, 44., 54.));
                row.add_child(
                    Container::new(Self::render_preview_cell(app, 44., 54.))
                        .with_margin_left(4.)
                        .finish(),
                );
                outer.add_child(row.finish());
            }
            TerminalLayoutPreset::TwoRows => {
                outer.add_child(Self::render_preview_cell(app, 92., 25.));
                outer.add_child(
                    Container::new(Self::render_preview_cell(app, 92., 25.))
                        .with_margin_top(4.)
                        .finish(),
                );
            }
            TerminalLayoutPreset::ThreeColumns => {
                let mut row = Flex::row().with_main_axis_size(MainAxisSize::Min);
                for index in 0..3 {
                    let cell = Self::render_preview_cell(app, 28., 54.);
                    row.add_child(if index == 0 {
                        cell
                    } else {
                        Container::new(cell).with_margin_left(4.).finish()
                    });
                }
                outer.add_child(row.finish());
            }
            TerminalLayoutPreset::Grid2x2 => {
                for row_index in 0..2 {
                    let mut row = Flex::row().with_main_axis_size(MainAxisSize::Min);
                    for column_index in 0..2 {
                        let cell = Self::render_preview_cell(app, 44., 25.);
                        row.add_child(if column_index == 0 {
                            cell
                        } else {
                            Container::new(cell).with_margin_left(4.).finish()
                        });
                    }
                    let row = row.finish();
                    outer.add_child(if row_index == 0 {
                        row
                    } else {
                        Container::new(row).with_margin_top(4.).finish()
                    });
                }
            }
        }

        outer.finish()
    }

    fn render_agent_mode_option(
        &self,
        agent_mode: TerminalLayoutAgentMode,
        selected_agent_mode: TerminalLayoutAgentMode,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let is_selected = agent_mode == selected_agent_mode;
        let background_color = if is_selected {
            theme.surface_3()
        } else {
            theme.surface_1()
        };
        let border_fill = if is_selected {
            theme.surface_3()
        } else {
            theme.outline()
        };
        let label_color = theme.active_ui_text_color().into();

        let option = Container::new(
            Text::new(agent_mode.label(), appearance.ui_font_family(), 13.)
                .with_color(label_color)
                .finish(),
        )
        .with_background(background_color)
        .with_border(warpui::elements::Border::all(1.).with_border_fill(border_fill))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(5.)))
        .with_padding_top(7.)
        .with_padding_right(14.)
        .with_padding_bottom(7.)
        .with_padding_left(14.)
        .finish();

        EventHandler::new(option)
            .on_left_mouse_down(move |ctx, _, _| {
                ctx.dispatch_typed_action(WorkspaceLayoutChooserAction::SetAgentMode(agent_mode));
                DispatchEventResult::StopPropagation
            })
            .finish()
    }

    fn render_agent_mode_selector(&self, app: &AppContext) -> Box<dyn Element> {
        let selected_agent_mode = self.selected_agent_mode;
        let mut row = Flex::row()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        for (index, agent_mode) in TerminalLayoutAgentMode::ALL.iter().copied().enumerate() {
            let option = self.render_agent_mode_option(agent_mode, selected_agent_mode, app);
            row.add_child(if index == 0 {
                option
            } else {
                Container::new(option).with_margin_left(8.).finish()
            });
        }

        row.finish()
    }

    fn render_preset_card(
        &self,
        preset: TerminalLayoutPreset,
        agent_mode: TerminalLayoutAgentMode,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();

        let mut content = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        content.add_child(self.render_preview(preset, app));
        content.add_child(
            Container::new(
                Text::new(preset.label(), appearance.ui_font_family(), 13.)
                    .with_color(theme.main_text_color(theme.surface_1()).into())
                    .finish(),
            )
            .with_margin_top(12.)
            .finish(),
        );
        content.add_child(
            Container::new(
                Text::new(preset.description(), appearance.ui_font_family(), 12.)
                    .with_color(theme.sub_text_color(theme.surface_1()).into())
                    .finish(),
            )
            .with_margin_top(4.)
            .finish(),
        );

        let card = Container::new(content.finish())
            .with_background(theme.surface_1())
            .with_border(warpui::elements::Border::all(1.).with_border_fill(theme.outline()))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
            .with_padding_top(16.)
            .with_padding_right(14.)
            .with_padding_bottom(14.)
            .with_padding_left(14.)
            .finish();

        EventHandler::new(ConstrainedBox::new(card).with_width(160.).finish())
            .on_left_mouse_down(move |ctx, _, _| {
                ctx.dispatch_typed_action(
                    WorkspaceAction::ApplyTerminalLayoutToActiveWorkspaceGroup {
                        preset,
                        agent_mode,
                    },
                );
                DispatchEventResult::StopPropagation
            })
            .finish()
    }
}

impl Entity for WorkspaceLayoutChooserView {
    type Event = PaneEvent;
}

impl TypedActionView for WorkspaceLayoutChooserView {
    type Action = WorkspaceLayoutChooserAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            WorkspaceLayoutChooserAction::SetAgentMode(agent_mode) => {
                self.selected_agent_mode = *agent_mode;
                ctx.notify();
            }
        }
    }
}

impl View for WorkspaceLayoutChooserView {
    fn ui_name() -> &'static str {
        "WorkspaceLayoutChooserView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();

        let mut column = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        column.add_child(
            Text::new("Choose terminal layout", appearance.ui_font_family(), 20.)
                .with_color(theme.main_text_color(theme.background()).into())
                .finish(),
        );
        column.add_child(
            Container::new(
                Text::new(
                    "Pick how this workspace should start.",
                    appearance.ui_font_family(),
                    13.,
                )
                .with_color(theme.sub_text_color(theme.background()).into())
                .finish(),
            )
            .with_margin_top(6.)
            .finish(),
        );
        column.add_child(
            Container::new(self.render_agent_mode_selector(app))
                .with_margin_top(18.)
                .finish(),
        );

        let mut card_grid = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        let agent_mode = self.selected_agent_mode;
        for (row_index, preset_row) in TerminalLayoutPreset::ALL.chunks(2).enumerate() {
            let mut row = Flex::row()
                .with_main_axis_size(MainAxisSize::Min)
                .with_main_axis_alignment(MainAxisAlignment::Center);
            for (column_index, preset) in preset_row.iter().copied().enumerate() {
                let card = self.render_preset_card(preset, agent_mode, app);
                row.add_child(if column_index == 0 {
                    card
                } else {
                    Container::new(card).with_margin_left(10.).finish()
                });
            }
            let row = row.finish();
            card_grid.add_child(if row_index == 0 {
                row
            } else {
                Container::new(row).with_margin_top(10.).finish()
            });
        }

        column.add_child(
            Container::new(card_grid.finish())
                .with_margin_top(24.)
                .finish(),
        );

        Align::new(
            ConstrainedBox::new(column.finish())
                .with_max_width(520.)
                .finish(),
        )
        .finish()
    }
}

impl BackingView for WorkspaceLayoutChooserView {
    type PaneHeaderOverflowMenuAction = ();
    type CustomAction = ();
    type AssociatedData = ();

    fn handle_pane_header_overflow_menu_action(
        &mut self,
        _action: &Self::PaneHeaderOverflowMenuAction,
        _ctx: &mut ViewContext<Self>,
    ) {
    }

    fn close(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(PaneEvent::Close);
    }

    fn focus_contents(&mut self, _ctx: &mut ViewContext<Self>) {}

    fn render_header_content(
        &self,
        _ctx: &view::HeaderRenderContext<'_>,
        _app: &AppContext,
    ) -> view::HeaderContent {
        view::HeaderContent::simple("Choose layout")
    }

    fn set_focus_handle(&mut self, focus_handle: PaneFocusHandle, ctx: &mut ViewContext<Self>) {
        self.set_focus_handle(focus_handle, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use warpui::{platform::WindowStyle, App};

    #[test]
    fn selector_action_updates_selected_agent_mode() {
        App::test((), |mut app| async move {
            app.add_singleton_model(|_| crate::appearance::Appearance::mock());
            let (_, chooser) =
                app.add_window(WindowStyle::NotStealFocus, WorkspaceLayoutChooserView::new);

            chooser.update(&mut app, |chooser, ctx| {
                assert_eq!(chooser.selected_agent_mode(), TerminalLayoutAgentMode::None);
                chooser.handle_action(
                    &WorkspaceLayoutChooserAction::SetAgentMode(TerminalLayoutAgentMode::Claude),
                    ctx,
                );
                assert_eq!(
                    chooser.selected_agent_mode(),
                    TerminalLayoutAgentMode::Claude
                );
                chooser.handle_action(
                    &WorkspaceLayoutChooserAction::SetAgentMode(TerminalLayoutAgentMode::Codex),
                    ctx,
                );
                assert_eq!(
                    chooser.selected_agent_mode(),
                    TerminalLayoutAgentMode::Codex
                );
            });
        });
    }
}
