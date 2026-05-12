use warpui::{AppContext, ModelHandle, View, ViewContext, ViewHandle};

use crate::{
    ai::blocklist::InputConfig,
    app_state::{LeafContents, TerminalPaneSnapshot},
    pane_group::{
        pane::{
            workspace_layout_chooser_view::WorkspaceLayoutChooserView, ShareableLink,
            ShareableLinkError,
        },
        BackingView, PaneConfiguration, PaneContent, PaneGroup, PaneView,
    },
};

use super::PaneId;

pub struct WorkspaceLayoutChooserPane {
    view: ViewHandle<PaneView<WorkspaceLayoutChooserView>>,
    pane_configuration: ModelHandle<PaneConfiguration>,
}

impl WorkspaceLayoutChooserPane {
    pub fn new<V: View>(ctx: &mut ViewContext<V>) -> Self {
        let chooser_view = ctx.add_typed_action_view(WorkspaceLayoutChooserView::new);
        let pane_configuration = chooser_view.as_ref(ctx).pane_configuration();
        let pane_view = ctx.add_typed_action_view(|ctx| {
            let pane_id = PaneId::from_workspace_layout_chooser_pane_ctx(ctx);
            PaneView::new(pane_id, chooser_view, (), pane_configuration.clone(), ctx)
        });
        Self {
            view: pane_view,
            pane_configuration,
        }
    }
}

impl PaneContent for WorkspaceLayoutChooserPane {
    fn id(&self) -> PaneId {
        PaneId::from_workspace_layout_chooser_pane_view(&self.view)
    }

    fn attach(
        &self,
        _group: &PaneGroup,
        focus_handle: crate::pane_group::focus_state::PaneFocusHandle,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        self.view
            .update(ctx, |view, ctx| view.set_focus_handle(focus_handle, ctx));

        let pane_id = self.id();
        let child = self.view.as_ref(ctx).child(ctx);
        ctx.subscribe_to_view(&child, move |pane_group, _, event, ctx| {
            pane_group.handle_pane_event(pane_id, event, ctx);
        });
    }

    fn detach(
        &self,
        _group: &PaneGroup,
        _detach_type: super::DetachType,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        let child = self.view.as_ref(ctx).child(ctx);
        ctx.unsubscribe_to_view(&child);
    }

    fn snapshot(&self, ctx: &AppContext) -> LeafContents {
        LeafContents::Terminal(TerminalPaneSnapshot {
            uuid: uuid::Uuid::new_v4().as_bytes().to_vec(),
            cwd: None,
            shell_launch_data: None,
            is_active: false,
            is_read_only: false,
            input_config: Some(InputConfig::new(ctx)),
            llm_model_override: None,
            active_profile_id: None,
            conversation_ids_to_restore: Vec::new(),
            active_conversation_id: None,
        })
    }

    fn has_application_focus(&self, ctx: &mut ViewContext<PaneGroup>) -> bool {
        self.view.is_self_or_child_focused(ctx)
    }

    fn focus(&self, ctx: &mut ViewContext<PaneGroup>) {
        self.view
            .as_ref(ctx)
            .child(ctx)
            .update(ctx, BackingView::focus_contents)
    }

    fn shareable_link(
        &self,
        _ctx: &mut ViewContext<PaneGroup>,
    ) -> Result<ShareableLink, ShareableLinkError> {
        Ok(ShareableLink::Base)
    }

    fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    fn is_pane_being_dragged(&self, ctx: &AppContext) -> bool {
        self.view.as_ref(ctx).is_being_dragged()
    }
}
