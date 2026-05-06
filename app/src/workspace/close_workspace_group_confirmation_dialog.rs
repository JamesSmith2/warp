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
        ActionButton, DangerPrimaryTheme, KeystrokeSource, NakedTheme,
    },
};

pub fn init(app: &mut AppContext) {
    use warpui::keymap::macros::*;

    app.register_fixed_bindings([
        FixedBinding::new(
            "escape",
            CloseWorkspaceGroupConfirmationAction::Cancel,
            id!(CloseWorkspaceGroupConfirmationDialog::ui_name()),
        ),
        FixedBinding::new(
            "enter",
            CloseWorkspaceGroupConfirmationAction::Confirm,
            id!(CloseWorkspaceGroupConfirmationDialog::ui_name()),
        ),
    ]);
}

const DIALOG_WIDTH: f32 = 460.;

#[derive(Clone, Debug)]
pub struct CloseWorkspaceGroupDialogSource {
    pub index: usize,
    pub name: String,
}

pub enum CloseWorkspaceGroupConfirmationEvent {
    Confirm {
        source: CloseWorkspaceGroupDialogSource,
    },
    Cancel,
}

#[derive(Debug)]
pub enum CloseWorkspaceGroupConfirmationAction {
    Confirm,
    Cancel,
}

pub struct CloseWorkspaceGroupConfirmationDialog {
    cancel_button: ViewHandle<ActionButton>,
    remove_button: ViewHandle<ActionButton>,
    source: Option<CloseWorkspaceGroupDialogSource>,
}

impl CloseWorkspaceGroupConfirmationDialog {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let cancel_button = ctx.add_typed_action_view(|_| {
            ActionButton::new("Cancel", NakedTheme).on_click(|ctx| {
                ctx.dispatch_typed_action(CloseWorkspaceGroupConfirmationAction::Cancel);
            })
        });

        let enter_keystroke = Keystroke::parse("enter").expect("Valid keystroke");
        let remove_button = ctx.add_typed_action_view(|ctx| {
            ActionButton::new("Remove", DangerPrimaryTheme)
                .with_keybinding(KeystrokeSource::Fixed(enter_keystroke), ctx)
                .on_click(|ctx| {
                    ctx.dispatch_typed_action(CloseWorkspaceGroupConfirmationAction::Confirm);
                })
        });

        Self {
            cancel_button,
            remove_button,
            source: None,
        }
    }

    pub fn set_source(&mut self, source: CloseWorkspaceGroupDialogSource) {
        self.source = Some(source);
    }
}

impl Entity for CloseWorkspaceGroupConfirmationDialog {
    type Event = CloseWorkspaceGroupConfirmationEvent;
}

impl View for CloseWorkspaceGroupConfirmationDialog {
    fn ui_name() -> &'static str {
        "CloseWorkspaceGroupConfirmationDialog"
    }

    fn on_focus(&mut self, _focus_ctx: &warpui::FocusContext, ctx: &mut ViewContext<Self>) {
        ctx.focus_self();
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);

        let cancel_button = Container::new(ChildView::new(&self.cancel_button).finish())
            .with_margin_right(12.)
            .finish();

        let title = self
            .source
            .as_ref()
            .map(|source| format!("Remove '{}'?", source.name))
            .unwrap_or_else(|| "Remove workspace?".to_string());

        let dialog = Dialog::new(
            title,
            Some("All tabs in this workspace will be closed. This action cannot be undone.".into()),
            UiComponentStyles {
                width: Some(DIALOG_WIDTH),
                ..dialog_styles(appearance)
            },
        )
        .with_bottom_row_child(cancel_button)
        .with_bottom_row_child(ChildView::new(&self.remove_button).finish())
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

impl TypedActionView for CloseWorkspaceGroupConfirmationDialog {
    type Action = CloseWorkspaceGroupConfirmationAction;

    fn handle_action(
        &mut self,
        action: &CloseWorkspaceGroupConfirmationAction,
        ctx: &mut ViewContext<Self>,
    ) {
        match action {
            CloseWorkspaceGroupConfirmationAction::Confirm => {
                let Some(source) = self.source.clone() else {
                    log::error!("Workspace remove confirm button pressed with no source");
                    return;
                };
                ctx.emit(CloseWorkspaceGroupConfirmationEvent::Confirm { source });
            }
            CloseWorkspaceGroupConfirmationAction::Cancel => {
                ctx.emit(CloseWorkspaceGroupConfirmationEvent::Cancel);
            }
        }
    }
}
