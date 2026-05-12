use pathfinder_geometry::vector::vec2f;
use warp_core::ui::theme::Fill;
use warpui::{
    elements::{
        Align, ChildAnchor, ChildView, Container, OffsetPositioning, ParentAnchor,
        ParentOffsetBounds, Stack,
    },
    keymap::{FixedBinding, Keystroke},
    ui_components::components::{UiComponent, UiComponentStyles},
    AppContext, Element, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle,
};

use crate::{
    appearance::Appearance,
    ui_components::dialog::{dialog_styles, Dialog},
    view_components::action_button::{
        ActionButton, DangerPrimaryTheme, KeystrokeSource, NakedTheme, PrimaryTheme,
    },
};

pub fn init(app: &mut AppContext) {
    use warpui::keymap::macros::*;

    app.register_fixed_bindings([
        FixedBinding::new(
            "escape",
            ClearWorkspacesConfirmationAction::Cancel,
            id!(ClearWorkspacesConfirmationDialog::ui_name()),
        ),
        FixedBinding::new(
            "enter",
            ClearWorkspacesConfirmationAction::ExportAndClear,
            id!(ClearWorkspacesConfirmationDialog::ui_name()),
        ),
    ]);
}

const DIALOG_WIDTH: f32 = 500.;

pub enum ClearWorkspacesConfirmationEvent {
    ExportAndClear,
    ClearWithoutExport,
    Cancel,
}

#[derive(Debug)]
pub enum ClearWorkspacesConfirmationAction {
    ExportAndClear,
    ClearWithoutExport,
    Cancel,
}

pub struct ClearWorkspacesConfirmationDialog {
    cancel_button: ViewHandle<ActionButton>,
    clear_without_export_button: ViewHandle<ActionButton>,
    export_and_clear_button: ViewHandle<ActionButton>,
}

impl ClearWorkspacesConfirmationDialog {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let cancel_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Cancel", NakedTheme).on_click(|ctx| {
                ctx.dispatch_typed_action(ClearWorkspacesConfirmationAction::Cancel);
            })
        });

        let clear_without_export_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Clear Without Export", DangerPrimaryTheme).on_click(|ctx| {
                ctx.dispatch_typed_action(ClearWorkspacesConfirmationAction::ClearWithoutExport);
            })
        });

        let enter_keystroke = Keystroke::parse("enter").expect("Valid keystroke");
        let export_and_clear_button = ctx.add_typed_action_view(|ctx| {
            ActionButton::new("Export & Clear", PrimaryTheme)
                .with_keybinding(KeystrokeSource::Fixed(enter_keystroke), ctx)
                .on_click(|ctx| {
                    ctx.dispatch_typed_action(ClearWorkspacesConfirmationAction::ExportAndClear);
                })
        });

        Self {
            cancel_button,
            clear_without_export_button,
            export_and_clear_button,
        }
    }
}

impl Entity for ClearWorkspacesConfirmationDialog {
    type Event = ClearWorkspacesConfirmationEvent;
}

impl View for ClearWorkspacesConfirmationDialog {
    fn ui_name() -> &'static str {
        "ClearWorkspacesConfirmationDialog"
    }

    fn on_focus(&mut self, _focus_ctx: &warpui::FocusContext, ctx: &mut ViewContext<Self>) {
        ctx.focus_self();
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);

        let cancel_button = Container::new(ChildView::new(&self.cancel_button).finish())
            .with_margin_right(12.)
            .finish();
        let clear_without_export_button =
            Container::new(ChildView::new(&self.clear_without_export_button).finish())
                .with_margin_right(12.)
                .finish();

        let dialog = Dialog::new(
            "Clear all workspaces?".to_string(),
            Some(
                "All tabs in all workspace windows will be closed. Each window will be replaced with one fresh workspace."
                    .into(),
            ),
            UiComponentStyles {
                width: Some(DIALOG_WIDTH),
                ..dialog_styles(appearance)
            },
        )
        .with_bottom_row_child(cancel_button)
        .with_bottom_row_child(clear_without_export_button)
        .with_bottom_row_child(ChildView::new(&self.export_and_clear_button).finish())
        .build()
        .finish();

        let mut stack = Stack::new();
        stack.add_positioned_child(
            dialog,
            OffsetPositioning::offset_from_parent(
                vec2f(0., 0.),
                ParentOffsetBounds::WindowByPosition,
                ParentAnchor::Center,
                ChildAnchor::Center,
            ),
        );

        Container::new(Align::new(stack.finish()).finish())
            .with_background_color(Fill::blur().into())
            .with_corner_radius(app.windows().window_corner_radius())
            .finish()
    }
}

impl TypedActionView for ClearWorkspacesConfirmationDialog {
    type Action = ClearWorkspacesConfirmationAction;

    fn handle_action(
        &mut self,
        action: &ClearWorkspacesConfirmationAction,
        ctx: &mut ViewContext<Self>,
    ) {
        match action {
            ClearWorkspacesConfirmationAction::ExportAndClear => {
                ctx.emit(ClearWorkspacesConfirmationEvent::ExportAndClear);
            }
            ClearWorkspacesConfirmationAction::ClearWithoutExport => {
                ctx.emit(ClearWorkspacesConfirmationEvent::ClearWithoutExport);
            }
            ClearWorkspacesConfirmationAction::Cancel => {
                ctx.emit(ClearWorkspacesConfirmationEvent::Cancel);
            }
        }
    }
}
