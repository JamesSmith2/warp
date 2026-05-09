//! Local rate-limit tracker for the Claude Code CLI.
//!
//! Claude Code does not expose an `account/rateLimits/read`-style RPC, so we
//! reconstruct usage from the transcript JSONL files Claude itself writes
//! under `$CLAUDE_CONFIG_DIR/projects/**/*.jsonl`. Each assistant entry in
//! those transcripts carries a `message.usage` block with token counts; we
//! sum the relevant counts into two rolling windows (5h "primary" and 7d
//! "secondary") and surface them via the same shape as
//! [`super::codex_rate_limits`] so the workspace resource panel can render
//! them through a shared helper.
//!
//! The percentage figures are approximations: Anthropic does not publish a
//! token-based cap and the actual server-side rate limit accounts for cost
//! across models. We hardcode plan caps that are roughly representative of
//! the Max5 plan today; a future pass should make this configurable.

use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use warpui::{r#async::Timer, Entity, ModelContext, SingletonEntity};

use crate::ai::agent_sdk::driver::harness::claude_transcript::claude_config_dir;

const REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const ERROR_BACKOFF_INTERVAL: Duration = Duration::from_secs(5 * 60);

const PRIMARY_WINDOW: chrono::Duration = chrono::Duration::hours(5);
const SECONDARY_WINDOW: chrono::Duration = chrono::Duration::days(7);
const PRIMARY_WINDOW_MINS: u32 = 300;
const SECONDARY_WINDOW_MINS: u32 = 7 * 24 * 60;

// TODO(claude-rate-limits): make these configurable per plan tier (Pro / Max5
// / Max20). For now we default to a Max5-shaped cap.
const PRIMARY_TOKEN_CAP: u64 = 35_000_000;
const SECONDARY_TOKEN_CAP: u64 = 140_000_000;

pub enum ClaudeRateLimitUsageModelEvent {
    Refreshed,
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeRateLimitUsage {
    pub current: Option<ClaudeRateLimitUsageSample>,
    pub last_error: Option<ClaudeRateLimitErrorState>,
    pub is_stale: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeRateLimitUsageSample {
    pub fetched_at: DateTime<Utc>,
    pub primary: Option<ClaudeRateLimitWindowUsage>,
    pub secondary: Option<ClaudeRateLimitWindowUsage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeRateLimitWindowUsage {
    pub kind: ClaudeRateLimitWindowKind,
    pub used_percent: f32,
    pub remaining_percent: f32,
    pub window_duration_mins: Option<u32>,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeRateLimitWindowKind {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeRateLimitErrorState {
    pub failed_at: DateTime<Utc>,
    pub message: String,
}

pub struct ClaudeRateLimitUsageModel {
    current: Option<ClaudeRateLimitUsageSample>,
    last_error: Option<ClaudeRateLimitErrorState>,
    refresh_in_flight: bool,
    last_refresh_requested_at: Option<Instant>,
}

impl ClaudeRateLimitUsageModel {
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            current: None,
            last_error: None,
            refresh_in_flight: false,
            last_refresh_requested_at: None,
        }
    }

    #[cfg(test)]
    pub fn new_for_test(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            current: None,
            last_error: None,
            refresh_in_flight: false,
            last_refresh_requested_at: None,
        }
    }

    pub fn usage(&self) -> ClaudeRateLimitUsage {
        let now = Utc::now();
        let is_stale = self.current.as_ref().is_some_and(|sample| {
            now.signed_duration_since(sample.fetched_at) > chrono::Duration::minutes(5)
        });

        ClaudeRateLimitUsage {
            current: self.current.clone(),
            last_error: self.last_error.clone(),
            is_stale,
        }
    }

    pub fn request_refresh(&mut self, ctx: &mut ModelContext<Self>) {
        if self.refresh_in_flight {
            return;
        }

        let now = Instant::now();
        let min_interval = if self.current.is_some() {
            REFRESH_INTERVAL
        } else {
            ERROR_BACKOFF_INTERVAL
        };
        if self
            .last_refresh_requested_at
            .is_some_and(|requested_at| requested_at.elapsed() < min_interval)
        {
            return;
        }

        self.refresh_in_flight = true;
        self.last_refresh_requested_at = Some(now);
        ctx.spawn(
            async move {
                Timer::after(Duration::ZERO).await;
                fetch_claude_rate_limits().await
            },
            |me, result, ctx| {
                me.refresh_in_flight = false;
                match result {
                    Ok(sample) => {
                        me.current = Some(sample);
                        me.last_error = None;
                    }
                    Err(error) => {
                        me.last_error = Some(ClaudeRateLimitErrorState {
                            failed_at: Utc::now(),
                            message: error.to_string(),
                        });
                    }
                };
                ctx.emit(ClaudeRateLimitUsageModelEvent::Refreshed);
            },
        );
    }
}

impl Entity for ClaudeRateLimitUsageModel {
    type Event = ClaudeRateLimitUsageModelEvent;
}

impl SingletonEntity for ClaudeRateLimitUsageModel {}

async fn fetch_claude_rate_limits() -> Result<ClaudeRateLimitUsageSample> {
    let config_root = claude_config_dir().context("could not resolve Claude config dir")?;
    let now = Utc::now();
    let totals = tokio::task::spawn_blocking(move || aggregate_usage(&config_root, now))
        .await
        .context("Claude rate-limit aggregation task panicked")??;
    let totals = totals.context("no recent Claude transcript usage found")?;
    Ok(build_sample(now, totals))
}

#[derive(Debug, Default, Clone, PartialEq)]
struct WindowTotals {
    /// Tokens summed across all assistant entries with `timestamp >= now - window`.
    tokens: u64,
    /// Earliest entry timestamp that fed into this bucket. Used to derive
    /// `resets_at`.
    oldest: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct AggregateTotals {
    primary: WindowTotals,
    secondary: WindowTotals,
}

fn aggregate_usage(config_root: &Path, now: DateTime<Utc>) -> Result<Option<AggregateTotals>> {
    let projects_dir = config_root.join("projects");
    if !projects_dir.is_dir() {
        return Ok(None);
    }

    let primary_cutoff = now - PRIMARY_WINDOW;
    let secondary_cutoff = now - SECONDARY_WINDOW;

    let mut totals = AggregateTotals::default();

    for project_entry in std::fs::read_dir(&projects_dir).with_context(|| {
        format!(
            "Failed to read Claude projects dir {}",
            projects_dir.display()
        )
    })? {
        let Ok(project_entry) = project_entry else {
            continue;
        };
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        collect_jsonl_files(&project_path, &mut |jsonl_path| {
            // Skip files whose mtime predates the secondary window — they
            // can't contribute to either bucket.
            if file_mtime(jsonl_path)
                .is_some_and(|mtime| mtime + chrono::Duration::minutes(1) < secondary_cutoff)
            {
                return;
            }
            if let Err(err) =
                accumulate_jsonl(jsonl_path, primary_cutoff, secondary_cutoff, &mut totals)
            {
                log::debug!(
                    "Skipping Claude transcript {} while sampling rate limits: {err:#}",
                    jsonl_path.display()
                );
            }
        });
    }

    Ok((totals.primary.oldest.is_some() || totals.secondary.oldest.is_some()).then_some(totals))
}

fn collect_jsonl_files(dir: &Path, on_file: &mut dyn FnMut(&Path)) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, on_file);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            on_file(&path);
        }
    }
}

fn file_mtime(path: &Path) -> Option<DateTime<Utc>> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    DateTime::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos())
}

fn accumulate_jsonl(
    path: &Path,
    primary_cutoff: DateTime<Utc>,
    secondary_cutoff: DateTime<Utc>,
    totals: &mut AggregateTotals,
) -> Result<()> {
    use std::io::BufRead as _;
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<TranscriptEntry>(&line) else {
            continue;
        };
        let Some(message) = entry.message else {
            continue;
        };
        let Some(usage) = message.usage else {
            continue;
        };
        let Some(timestamp) = entry.timestamp else {
            continue;
        };

        if timestamp < secondary_cutoff {
            continue;
        }

        let tokens = usage.total_billable_tokens();

        record_window(&mut totals.secondary, timestamp, tokens);
        if timestamp >= primary_cutoff {
            record_window(&mut totals.primary, timestamp, tokens);
        }
    }
    Ok(())
}

fn record_window(window: &mut WindowTotals, timestamp: DateTime<Utc>, tokens: u64) {
    window.tokens = window.tokens.saturating_add(tokens);
    window.oldest = Some(match window.oldest {
        Some(existing) if existing <= timestamp => existing,
        _ => timestamp,
    });
}

fn build_sample(now: DateTime<Utc>, totals: AggregateTotals) -> ClaudeRateLimitUsageSample {
    ClaudeRateLimitUsageSample {
        fetched_at: now,
        primary: Some(build_window(
            ClaudeRateLimitWindowKind::Primary,
            totals.primary,
            PRIMARY_TOKEN_CAP,
            PRIMARY_WINDOW,
            PRIMARY_WINDOW_MINS,
        )),
        secondary: Some(build_window(
            ClaudeRateLimitWindowKind::Secondary,
            totals.secondary,
            SECONDARY_TOKEN_CAP,
            SECONDARY_WINDOW,
            SECONDARY_WINDOW_MINS,
        )),
    }
}

fn build_window(
    kind: ClaudeRateLimitWindowKind,
    totals: WindowTotals,
    cap_tokens: u64,
    window_duration: chrono::Duration,
    window_duration_mins: u32,
) -> ClaudeRateLimitWindowUsage {
    let used_percent = if cap_tokens == 0 {
        0.
    } else {
        ((totals.tokens as f64 / cap_tokens as f64) * 100.).min(100.) as f32
    };
    let remaining_percent = (100. - used_percent).clamp(0., 100.);
    let resets_at = totals.oldest.map(|oldest| oldest + window_duration);
    ClaudeRateLimitWindowUsage {
        kind,
        used_percent,
        remaining_percent,
        window_duration_mins: Some(window_duration_mins),
        resets_at,
    }
}

#[derive(Debug, Deserialize)]
struct TranscriptEntry {
    #[serde(default)]
    timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    message: Option<TranscriptMessage>,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessage {
    #[serde(default)]
    usage: Option<TranscriptUsage>,
}

#[derive(Debug, Deserialize)]
struct TranscriptUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

impl TranscriptUsage {
    fn total_billable_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_creation_input_tokens)
            .saturating_add(self.cache_read_input_tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use std::io::Write as _;
    use tempfile::TempDir;

    fn write_jsonl(dir: &Path, name: &str, lines: &[&str]) {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    fn assistant_line(ts: DateTime<Utc>, input: u64, output: u64) -> String {
        format!(
            "{{\"timestamp\":\"{}\",\"message\":{{\"usage\":{{\"input_tokens\":{input},\"output_tokens\":{output}}}}}}}",
            ts.to_rfc3339()
        )
    }

    #[test]
    fn missing_projects_dir_yields_no_sample() {
        let tmp = TempDir::new().unwrap();
        let now = Utc::now();
        let totals = aggregate_usage(tmp.path(), now).unwrap();
        assert_eq!(totals, None);
    }

    #[test]
    fn aggregates_tokens_across_windows() {
        let tmp = TempDir::new().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
        let inside_primary = now - chrono::Duration::hours(2);
        let outside_primary = now - chrono::Duration::hours(8);
        let outside_secondary = now - chrono::Duration::days(10);
        let project = tmp.path().join("projects").join("-Users-test-project");
        write_jsonl(
            &project,
            "session.jsonl",
            &[
                &assistant_line(inside_primary, 100, 50),
                &assistant_line(outside_primary, 200, 100),
                &assistant_line(outside_secondary, 9_999, 9_999),
            ],
        );

        let totals = aggregate_usage(tmp.path(), now).unwrap().unwrap();
        // Primary picks up only the entry within the last 5 hours.
        assert_eq!(totals.primary.tokens, 100 + 50);
        assert_eq!(totals.primary.oldest, Some(inside_primary));
        // Secondary picks up entries within the last 7 days.
        assert_eq!(totals.secondary.tokens, 100 + 50 + 200 + 100);
        assert_eq!(totals.secondary.oldest, Some(outside_primary));
    }

    #[test]
    fn ignores_lines_without_usage() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-project");
        let now = Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
        let inside = now - chrono::Duration::hours(1);
        write_jsonl(
            &project,
            "session.jsonl",
            &[
                &format!(
                    "{{\"timestamp\":\"{}\",\"message\":{{\"id\":\"x\"}}}}",
                    inside.to_rfc3339()
                ),
                &assistant_line(inside, 10, 5),
            ],
        );
        let totals = aggregate_usage(tmp.path(), now).unwrap().unwrap();
        assert_eq!(totals.primary.tokens, 15);
    }

    #[test]
    fn recurses_into_subagent_dirs() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-project");
        let subagents = project
            .join("00000000-0000-0000-0000-000000000000")
            .join("subagents");
        let now = Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
        let inside = now - chrono::Duration::hours(1);
        write_jsonl(&project, "main.jsonl", &[&assistant_line(inside, 10, 0)]);
        write_jsonl(&subagents, "agent.jsonl", &[&assistant_line(inside, 5, 0)]);
        let totals = aggregate_usage(tmp.path(), now).unwrap().unwrap();
        assert_eq!(totals.primary.tokens, 15);
    }

    #[test]
    fn usage_outside_windows_yields_no_sample() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-project");
        let now = Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
        let outside_secondary = now - chrono::Duration::days(10);
        write_jsonl(
            &project,
            "session.jsonl",
            &[&assistant_line(outside_secondary, 10, 5)],
        );

        assert_eq!(aggregate_usage(tmp.path(), now).unwrap(), None);
    }

    #[test]
    fn build_window_clamps_above_cap() {
        let oldest = Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
        let totals = WindowTotals {
            tokens: 999_999_999,
            oldest: Some(oldest),
        };
        let window = build_window(
            ClaudeRateLimitWindowKind::Primary,
            totals,
            PRIMARY_TOKEN_CAP,
            PRIMARY_WINDOW,
            PRIMARY_WINDOW_MINS,
        );
        assert_eq!(window.used_percent, 100.);
        assert_eq!(window.remaining_percent, 0.);
        assert_eq!(window.resets_at, Some(oldest + PRIMARY_WINDOW));
    }
}
