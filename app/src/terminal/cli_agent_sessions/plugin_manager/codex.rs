use std::sync::LazyLock;

use async_trait::async_trait;

use super::{CliAgentPluginManager, PluginInstructionStep, PluginInstructions};

pub(super) struct CodexPluginManager;

const CODEX_WARP_HOOKS_JSON: &str = r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "printf '\\033]777;notify;warp://cli-agent;%s\\007' '{\"v\":1,\"agent\":\"codex\",\"event\":\"session_start\"}' >/dev/tty 2>/dev/null || true"
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "printf '\\033]777;notify;warp://cli-agent;%s\\007' '{\"v\":1,\"agent\":\"codex\",\"event\":\"prompt_submit\"}' >/dev/tty 2>/dev/null || true"
          }
        ]
      }
    ],
    "PermissionRequest": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "printf '\\033]777;notify;warp://cli-agent;%s\\007' '{\"v\":1,\"agent\":\"codex\",\"event\":\"permission_request\",\"summary\":\"Codex is waiting for approval\"}' >/dev/tty 2>/dev/null || true"
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "printf '\\033]777;notify;warp://cli-agent;%s\\007' '{\"v\":1,\"agent\":\"codex\",\"event\":\"permission_replied\"}' >/dev/tty 2>/dev/null || true"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "printf '\\033]777;notify;warp://cli-agent;%s\\007' '{\"v\":1,\"agent\":\"codex\",\"event\":\"tool_complete\"}' >/dev/tty 2>/dev/null || true"
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "printf '\\033]777;notify;warp://cli-agent;%s\\007' '{\"v\":1,\"agent\":\"codex\",\"event\":\"stop\"}' >/dev/tty 2>/dev/null || true"
          }
        ]
      }
    ]
  }
}"#;

const CODEX_CONFIG_SNIPPET: &str = r#"[features]
hooks = true

[tui]
notification_condition = "always""#;

#[async_trait]
impl CliAgentPluginManager for CodexPluginManager {
    fn minimum_plugin_version(&self) -> &'static str {
        "0.0.0"
    }

    fn can_auto_install(&self) -> bool {
        false
    }

    fn supports_update(&self) -> bool {
        false
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        &INSTALL_INSTRUCTIONS
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        &EMPTY_INSTRUCTIONS
    }
}

static INSTALL_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| {
    PluginInstructions {
        title: "Enable Warp Notifications for Codex",
        subtitle: "Configure Codex hooks so Warp can distinguish an active Codex turn from an idle open Codex TUI.",
        steps: &[
            PluginInstructionStep {
                description: "Update Codex to the latest version.",
                command: "",
                executable: false,
                link: Some("https://developers.openai.com/codex/cli#upgrade"),
            },
            PluginInstructionStep {
                description: "Create ~/.codex/hooks.json with these hooks, or merge the hooks into your existing file.",
                command: CODEX_WARP_HOOKS_JSON,
                executable: false,
                link: None,
            },
            PluginInstructionStep {
                description: "Enable Codex hooks and keep in-focus notifications enabled. If your config already has [features] or [tui] sections, add these keys there instead of creating duplicate sections.",
                command: CODEX_CONFIG_SNIPPET,
                executable: false,
                link: None,
            },
        ],
        post_install_notes: &[
            "Restart Codex to apply the changes.",
            "If Codex asks you to review hooks, run /hooks and trust the Warp notification hooks.",
        ],
    }
});

static EMPTY_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: "",
    subtitle: "",
    steps: &[],
    post_install_notes: &[],
});

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
