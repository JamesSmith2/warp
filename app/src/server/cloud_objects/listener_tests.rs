use std::sync::Arc;

use warpui::{AddSingletonModel, App};

use super::*;
use crate::{
    auth::AuthStateProvider, cloud_object::model::persistence::CloudModel, network::NetworkStatus,
    server::server_api::object::MockObjectClient, system::SystemStats,
    workspaces::user_workspaces::UserWorkspaces,
};

fn install_required_singletons(app: &mut App, auth_state_provider: AuthStateProvider) {
    app.add_singleton_model(|_| NetworkStatus::new());
    app.add_singleton_model(|_| SystemStats::new());
    app.add_singleton_model(|_| auth_state_provider);
    app.add_singleton_model(CloudModel::mock);
    app.add_singleton_model(UserWorkspaces::default_mock);
}

#[test]
fn start_listener_skips_websocket_when_logged_out() {
    App::test((), |mut app| async move {
        install_required_singletons(&mut app, AuthStateProvider::new_logged_out_for_test());

        let mut object_client = MockObjectClient::new();
        object_client.expect_get_warp_drive_updates().times(0);
        let object_client = Arc::new(object_client);
        let listener = app.add_singleton_model(|ctx| Listener::new(object_client, ctx));

        listener.update(&mut app, |listener, ctx| {
            listener.start_listener(ctx);
        });

        listener.read(&app, |listener, _| {
            assert!(!listener.should_subscribe_to_updates);
            assert!(!listener.has_current_subscription_abort_handle());
        });
    });
}

#[test]
fn start_listener_starts_websocket_when_logged_in() {
    App::test((), |mut app| async move {
        install_required_singletons(&mut app, AuthStateProvider::new_for_test());

        let mut object_client = MockObjectClient::new();
        object_client
            .expect_get_warp_drive_updates()
            .returning(|_, _| Ok(()));
        let object_client = Arc::new(object_client);
        let listener = app.add_singleton_model(|ctx| Listener::new(object_client, ctx));

        listener.update(&mut app, |listener, ctx| {
            listener.start_listener(ctx);
        });

        listener.read(&app, |listener, _| {
            assert!(listener.should_subscribe_to_updates);
            assert!(listener.has_current_subscription_abort_handle());
        });
    });
}
