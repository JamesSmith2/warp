use super::CodexPluginManager;
use crate::terminal::cli_agent_sessions::plugin_manager::CliAgentPluginManager;
use serde_json::Value;

#[test]
fn can_auto_install_is_false() {
    assert!(!CodexPluginManager.can_auto_install());
}

#[test]
fn does_not_support_update() {
    assert!(!CodexPluginManager.supports_update());
}

#[test]
fn install_instructions_has_steps() {
    let instructions = CodexPluginManager.install_instructions();
    assert!(!instructions.steps.is_empty());
    assert!(!instructions.title.is_empty());
}

#[test]
fn install_instructions_emit_warp_structured_events() {
    let instructions = CodexPluginManager.install_instructions();
    let rendered_commands = instructions
        .steps
        .iter()
        .map(|step| step.command)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered_commands.contains("warp://cli-agent"));
    assert!(rendered_commands.contains("\"hooks\""));
    assert!(rendered_commands.contains("UserPromptSubmit"));
    assert!(rendered_commands.contains("prompt_submit"));
    assert!(rendered_commands.contains("PermissionRequest"));
    assert!(rendered_commands.contains("permission_request"));
    assert!(rendered_commands.contains("PreToolUse"));
    assert!(rendered_commands.contains("permission_replied"));
    assert!(rendered_commands.contains("PostToolUse"));
    assert!(rendered_commands.contains("tool_complete"));
    assert!(rendered_commands.contains("Stop"));
    assert!(rendered_commands.contains("\\\"event\\\":\\\"stop\\\""));
    assert!(rendered_commands.contains("hooks = true"));
}

#[test]
fn install_instructions_include_valid_hooks_json() {
    let json = serde_json::from_str::<Value>(super::CODEX_WARP_HOOKS_JSON).unwrap();

    let prompt_submit_command = json
        .pointer("/hooks/UserPromptSubmit/0/hooks/0/command")
        .and_then(Value::as_str)
        .unwrap();
    let stop_command = json
        .pointer("/hooks/Stop/0/hooks/0/command")
        .and_then(Value::as_str)
        .unwrap();
    let permission_request_command = json
        .pointer("/hooks/PermissionRequest/0/hooks/0/command")
        .and_then(Value::as_str)
        .unwrap();
    let pre_tool_use_command = json
        .pointer("/hooks/PreToolUse/0/hooks/0/command")
        .and_then(Value::as_str)
        .unwrap();
    let post_tool_use_command = json
        .pointer("/hooks/PostToolUse/0/hooks/0/command")
        .and_then(Value::as_str)
        .unwrap();

    assert!(prompt_submit_command.contains("warp://cli-agent"));
    assert!(prompt_submit_command.contains("prompt_submit"));
    assert!(permission_request_command.contains("permission_request"));
    assert!(pre_tool_use_command.contains("permission_replied"));
    assert!(post_tool_use_command.contains("tool_complete"));
    assert!(stop_command.contains("stop"));
}
