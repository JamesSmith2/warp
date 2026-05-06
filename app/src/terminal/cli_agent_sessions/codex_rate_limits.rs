use std::collections::VecDeque;
use std::time::Duration;

use anyhow::{anyhow, Context as _, Result};
use chrono::{DateTime, Utc};
use command::{r#async::Command, Stdio};
use futures::{
    future::FutureExt as _,
    io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader, BufWriter},
    pin_mut, select,
};
use serde::Deserialize;
use serde_json::json;
use warpui::{r#async::Timer, Entity, ModelContext, SingletonEntity};

const REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const ERROR_BACKOFF_INTERVAL: Duration = Duration::from_secs(5 * 60);
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const HISTORY_WINDOW: chrono::Duration = chrono::Duration::minutes(30);
const MIN_ESTIMATION_SAMPLE_COUNT: usize = 3;
const MIN_ESTIMATION_SPAN_MINS: f64 = 5.;
const FLAT_SLOPE_THRESHOLD_PERCENT_PER_MIN: f64 = 0.05;
const RESET_REMAINING_INCREASE_THRESHOLD: f32 = 5.;

pub enum CodexRateLimitUsageModelEvent {
    Refreshed,
}

#[derive(Debug, Clone, Default)]
pub struct CodexRateLimitUsage {
    pub current: Option<CodexRateLimitUsageSample>,
    pub history: Vec<CodexRateLimitUsageSample>,
    pub last_error: Option<CodexRateLimitErrorState>,
    pub is_stale: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodexRateLimitUsageSample {
    pub fetched_at: DateTime<Utc>,
    pub primary: Option<CodexRateLimitWindowUsage>,
    pub secondary: Option<CodexRateLimitWindowUsage>,
    pub plan_type: Option<String>,
    pub rate_limit_reached_type: Option<String>,
}

impl CodexRateLimitUsageSample {
    fn window(&self, kind: CodexRateLimitWindowKind) -> Option<&CodexRateLimitWindowUsage> {
        match kind {
            CodexRateLimitWindowKind::Primary => self.primary.as_ref(),
            CodexRateLimitWindowKind::Secondary => self.secondary.as_ref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodexRateLimitWindowUsage {
    pub kind: CodexRateLimitWindowKind,
    pub used_percent: f32,
    pub remaining_percent: f32,
    pub window_duration_mins: Option<u32>,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexRateLimitWindowKind {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexRateLimitErrorState {
    pub failed_at: DateTime<Utc>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CodexRateLimitProjection {
    EmptyNow,
    EmptyAt(DateTime<Utc>),
    ResetsAt(DateTime<Utc>),
    Stable,
    Unknown,
}

pub struct CodexRateLimitUsageModel {
    history: CodexRateLimitHistoryBuffer,
    current: Option<CodexRateLimitUsageSample>,
    last_error: Option<CodexRateLimitErrorState>,
}

impl CodexRateLimitUsageModel {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        Self::schedule_refresh(ctx, Duration::ZERO);
        Self {
            history: Default::default(),
            current: None,
            last_error: None,
        }
    }

    #[cfg(test)]
    pub fn new_for_test(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            history: Default::default(),
            current: None,
            last_error: None,
        }
    }

    pub fn usage(&self) -> CodexRateLimitUsage {
        let now = Utc::now();
        let is_stale = self.current.as_ref().is_some_and(|sample| {
            now.signed_duration_since(sample.fetched_at) > chrono::Duration::minutes(5)
        });

        CodexRateLimitUsage {
            current: self.current.clone(),
            history: self.history.iter().cloned().collect(),
            last_error: self.last_error.clone(),
            is_stale,
        }
    }

    fn schedule_refresh(ctx: &mut ModelContext<Self>, delay: Duration) {
        ctx.spawn(
            async move {
                Timer::after(delay).await;
                fetch_codex_rate_limits_with_timeout().await
            },
            |me, result, ctx| {
                let next_delay = match result {
                    Ok(sample) => {
                        me.history.push(sample.clone());
                        me.current = Some(sample);
                        me.last_error = None;
                        REFRESH_INTERVAL
                    }
                    Err(error) => {
                        me.last_error = Some(CodexRateLimitErrorState {
                            failed_at: Utc::now(),
                            message: error.to_string(),
                        });
                        ERROR_BACKOFF_INTERVAL
                    }
                };

                ctx.emit(CodexRateLimitUsageModelEvent::Refreshed);
                Self::schedule_refresh(ctx, next_delay);
            },
        );
    }
}

impl Entity for CodexRateLimitUsageModel {
    type Event = CodexRateLimitUsageModelEvent;
}

impl SingletonEntity for CodexRateLimitUsageModel {}

#[derive(Default)]
struct CodexRateLimitHistoryBuffer {
    samples: VecDeque<CodexRateLimitUsageSample>,
}

impl CodexRateLimitHistoryBuffer {
    fn push(&mut self, sample: CodexRateLimitUsageSample) {
        let cutoff = sample.fetched_at - HISTORY_WINDOW;
        self.samples.push_back(sample);
        while self
            .samples
            .front()
            .is_some_and(|front| front.fetched_at < cutoff)
        {
            self.samples.pop_front();
        }
    }

    fn iter(&self) -> impl Iterator<Item = &CodexRateLimitUsageSample> {
        self.samples.iter()
    }
}

pub fn estimate_codex_rate_limit_projection(
    usage: &CodexRateLimitUsage,
    now: DateTime<Utc>,
) -> CodexRateLimitProjection {
    let Some(current) = usage.current.as_ref() else {
        return if usage.last_error.is_some() {
            CodexRateLimitProjection::Unknown
        } else {
            CodexRateLimitProjection::Stable
        };
    };

    if current.rate_limit_reached_type.is_some()
        || current
            .primary
            .as_ref()
            .is_some_and(|window| window.remaining_percent <= 0.)
        || current
            .secondary
            .as_ref()
            .is_some_and(|window| window.remaining_percent <= 0.)
    {
        return CodexRateLimitProjection::EmptyNow;
    }

    let projections = [
        estimate_window_projection(
            &usage.history,
            current,
            CodexRateLimitWindowKind::Primary,
            now,
        ),
        estimate_window_projection(
            &usage.history,
            current,
            CodexRateLimitWindowKind::Secondary,
            now,
        ),
    ];

    if let Some(empty_at) = projections
        .iter()
        .filter_map(|projection| match projection {
            CodexRateLimitProjection::EmptyAt(empty_at) => Some(*empty_at),
            _ => None,
        })
        .min()
    {
        return CodexRateLimitProjection::EmptyAt(empty_at);
    }

    if let Some(resets_at) = projections
        .iter()
        .filter_map(|projection| match projection {
            CodexRateLimitProjection::ResetsAt(resets_at) => Some(*resets_at),
            _ => None,
        })
        .min()
    {
        return CodexRateLimitProjection::ResetsAt(resets_at);
    }

    if projections
        .iter()
        .any(|projection| matches!(projection, CodexRateLimitProjection::Stable))
    {
        CodexRateLimitProjection::Stable
    } else {
        CodexRateLimitProjection::Unknown
    }
}

fn estimate_window_projection(
    history: &[CodexRateLimitUsageSample],
    current: &CodexRateLimitUsageSample,
    kind: CodexRateLimitWindowKind,
    now: DateTime<Utc>,
) -> CodexRateLimitProjection {
    let Some(current_window) = current.window(kind) else {
        return CodexRateLimitProjection::Unknown;
    };

    if current_window.remaining_percent <= 0. {
        return CodexRateLimitProjection::EmptyNow;
    }

    let points = current_window_segment_points(history, current, kind);
    if points.len() < MIN_ESTIMATION_SAMPLE_COUNT {
        return current_window
            .resets_at
            .map(CodexRateLimitProjection::ResetsAt)
            .unwrap_or(CodexRateLimitProjection::Unknown);
    }

    let span_mins = points
        .last()
        .zip(points.first())
        .map(|(last, first)| {
            last.0.signed_duration_since(first.0).num_seconds().max(0) as f64 / 60.
        })
        .unwrap_or_default();
    if span_mins < MIN_ESTIMATION_SPAN_MINS {
        return current_window
            .resets_at
            .map(CodexRateLimitProjection::ResetsAt)
            .unwrap_or(CodexRateLimitProjection::Unknown);
    }

    let slope = remaining_percent_slope_per_min(&points);
    if slope >= -FLAT_SLOPE_THRESHOLD_PERCENT_PER_MIN {
        return current_window
            .resets_at
            .map(CodexRateLimitProjection::ResetsAt)
            .unwrap_or(CodexRateLimitProjection::Stable);
    }

    let minutes_to_empty = current_window.remaining_percent as f64 / -slope;
    let empty_at = now + chrono::Duration::seconds((minutes_to_empty * 60.).round() as i64);

    if current_window
        .resets_at
        .is_some_and(|resets_at| empty_at >= resets_at)
    {
        return CodexRateLimitProjection::ResetsAt(current_window.resets_at.unwrap());
    }

    CodexRateLimitProjection::EmptyAt(empty_at)
}

fn current_window_segment_points(
    history: &[CodexRateLimitUsageSample],
    current: &CodexRateLimitUsageSample,
    kind: CodexRateLimitWindowKind,
) -> Vec<(DateTime<Utc>, f32)> {
    let current_window = current
        .window(kind)
        .expect("current window exists before segmenting");
    let current_resets_at = current_window.resets_at;
    let mut reversed_points = Vec::new();
    let mut previous_remaining: Option<f32> = None;

    for sample in history.iter().rev() {
        let Some(window) = sample.window(kind) else {
            continue;
        };

        if current_resets_at.is_some() && window.resets_at != current_resets_at {
            break;
        }

        if let Some(previous_remaining) = previous_remaining {
            if previous_remaining - window.remaining_percent > RESET_REMAINING_INCREASE_THRESHOLD {
                break;
            }
        }

        reversed_points.push((sample.fetched_at, window.remaining_percent));
        previous_remaining = Some(window.remaining_percent);
    }

    reversed_points.reverse();
    reversed_points
}

fn remaining_percent_slope_per_min(points: &[(DateTime<Utc>, f32)]) -> f64 {
    let first = points
        .first()
        .map(|point| point.0)
        .expect("slope requires at least one point");
    let points = points
        .iter()
        .map(|(timestamp, remaining)| {
            (
                timestamp.signed_duration_since(first).num_seconds() as f64 / 60.,
                *remaining as f64,
            )
        })
        .collect::<Vec<_>>();

    let count = points.len() as f64;
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / count;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / count;
    let variance_x = points
        .iter()
        .map(|(x, _)| {
            let delta = x - mean_x;
            delta * delta
        })
        .sum::<f64>();

    if variance_x == 0. {
        return 0.;
    }

    points
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum::<f64>()
        / variance_x
}

async fn fetch_codex_rate_limits_with_timeout() -> Result<CodexRateLimitUsageSample> {
    let fetch = fetch_codex_rate_limits().fuse();
    let timeout = Timer::after(PROBE_TIMEOUT).fuse();
    pin_mut!(fetch);
    pin_mut!(timeout);

    select! {
        result = fetch => result,
        _ = timeout => Err(anyhow!("Codex rate-limit probe timed out")),
    }
}

async fn fetch_codex_rate_limits() -> Result<CodexRateLimitUsageSample> {
    let mut command = Command::new("codex");
    command
        .args(["app-server", "--listen", "stdio://"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .context("failed to start Codex app server")?;
    let stdin = child
        .stdin
        .take()
        .context("Codex app server stdin was not available")?;
    let stdout = child
        .stdout
        .take()
        .context("Codex app server stdout was not available")?;

    let mut writer = BufWriter::new(stdin);
    let initialize = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "warp-codex-rate-limit-monitor",
                "title": null,
                "version": "0.0.0"
            },
            "capabilities": null
        }
    });
    let read_limits = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "account/rateLimits/read"
    });

    writer.write_all(initialize.to_string().as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.write_all(read_limits.to_string().as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }

        let Ok(response) = serde_json::from_str::<CodexJsonRpcResponse>(&line) else {
            continue;
        };
        if response.id != Some(2) {
            continue;
        }
        let _ = child.kill();
        return response.into_usage_sample(Utc::now());
    }

    let _ = child.kill();
    Err(anyhow!(
        "Codex app server exited before returning rate limits"
    ))
}

#[derive(Debug, Deserialize)]
struct CodexJsonRpcResponse {
    id: Option<u64>,
    result: Option<CodexRateLimitsReadResult>,
    error: Option<CodexJsonRpcError>,
}

impl CodexJsonRpcResponse {
    fn into_usage_sample(self, fetched_at: DateTime<Utc>) -> Result<CodexRateLimitUsageSample> {
        if let Some(error) = self.error {
            return Err(anyhow!(
                "Codex rate-limit request failed: {}",
                error.message.unwrap_or_else(|| error.code.to_string())
            ));
        }

        self.result
            .context("Codex rate-limit response did not include a result")?
            .into_usage_sample(fetched_at)
    }
}

#[derive(Debug, Deserialize)]
struct CodexJsonRpcError {
    code: i64,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexRateLimitsReadResult {
    rate_limits: CodexRateLimitSnapshot,
}

impl CodexRateLimitsReadResult {
    fn into_usage_sample(self, fetched_at: DateTime<Utc>) -> Result<CodexRateLimitUsageSample> {
        Ok(CodexRateLimitUsageSample {
            fetched_at,
            primary: self
                .rate_limits
                .primary
                .map(|window| window.into_usage(CodexRateLimitWindowKind::Primary)),
            secondary: self
                .rate_limits
                .secondary
                .map(|window| window.into_usage(CodexRateLimitWindowKind::Secondary)),
            plan_type: self.rate_limits.plan_type,
            rate_limit_reached_type: self.rate_limits.rate_limit_reached_type,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexRateLimitSnapshot {
    primary: Option<CodexRateLimitWindow>,
    secondary: Option<CodexRateLimitWindow>,
    plan_type: Option<String>,
    rate_limit_reached_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexRateLimitWindow {
    used_percent: f32,
    window_duration_mins: Option<u32>,
    resets_at: Option<i64>,
}

impl CodexRateLimitWindow {
    fn into_usage(self, kind: CodexRateLimitWindowKind) -> CodexRateLimitWindowUsage {
        let used_percent = self.used_percent.clamp(0., 100.);
        CodexRateLimitWindowUsage {
            kind,
            used_percent,
            remaining_percent: (100. - used_percent).clamp(0., 100.),
            window_duration_mins: self.window_duration_mins,
            resets_at: self
                .resets_at
                .and_then(|timestamp| DateTime::<Utc>::from_timestamp(timestamp, 0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        fetched_at: DateTime<Utc>,
        remaining_percent: f32,
        resets_at: Option<DateTime<Utc>>,
    ) -> CodexRateLimitUsageSample {
        CodexRateLimitUsageSample {
            fetched_at,
            primary: Some(CodexRateLimitWindowUsage {
                kind: CodexRateLimitWindowKind::Primary,
                used_percent: 100. - remaining_percent,
                remaining_percent,
                window_duration_mins: Some(300),
                resets_at,
            }),
            secondary: None,
            plan_type: Some("pro".to_string()),
            rate_limit_reached_type: None,
        }
    }

    #[test]
    fn parses_rate_limit_response() {
        let response = r#"{
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "rateLimits": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 21,
                        "windowDurationMins": 300,
                        "resetsAt": 1777880803
                    },
                    "secondary": {
                        "usedPercent": 80,
                        "windowDurationMins": 10080,
                        "resetsAt": 1777960950
                    },
                    "credits": {
                        "hasCredits": false,
                        "unlimited": false,
                        "balance": "0"
                    },
                    "planType": "pro",
                    "rateLimitReachedType": null
                },
                "rateLimitsByLimitId": null
            }
        }"#;

        let response = serde_json::from_str::<CodexJsonRpcResponse>(response).unwrap();
        let sample = response.into_usage_sample(DateTime::UNIX_EPOCH).unwrap();

        assert_eq!(
            sample
                .primary
                .as_ref()
                .map(|window| window.remaining_percent),
            Some(79.)
        );
        assert_eq!(
            sample
                .secondary
                .as_ref()
                .map(|window| window.remaining_percent),
            Some(20.)
        );
        assert_eq!(sample.plan_type.as_deref(), Some("pro"));
    }

    #[test]
    fn estimates_empty_before_reset_from_declining_history() {
        let base = DateTime::UNIX_EPOCH;
        let reset = base + chrono::Duration::hours(3);
        let history = vec![
            sample(base, 30., Some(reset)),
            sample(base + chrono::Duration::minutes(10), 20., Some(reset)),
            sample(base + chrono::Duration::minutes(20), 10., Some(reset)),
        ];
        let usage = CodexRateLimitUsage {
            current: history.last().cloned(),
            history,
            last_error: None,
            is_stale: false,
        };

        assert_eq!(
            estimate_codex_rate_limit_projection(&usage, base + chrono::Duration::minutes(20)),
            CodexRateLimitProjection::EmptyAt(base + chrono::Duration::minutes(30))
        );
    }

    #[test]
    fn prefers_reset_when_reset_happens_before_empty() {
        let base = DateTime::UNIX_EPOCH;
        let reset = base + chrono::Duration::minutes(25);
        let history = vec![
            sample(base, 30., Some(reset)),
            sample(base + chrono::Duration::minutes(10), 25., Some(reset)),
            sample(base + chrono::Duration::minutes(20), 20., Some(reset)),
        ];
        let usage = CodexRateLimitUsage {
            current: history.last().cloned(),
            history,
            last_error: None,
            is_stale: false,
        };

        assert_eq!(
            estimate_codex_rate_limit_projection(&usage, base + chrono::Duration::minutes(20)),
            CodexRateLimitProjection::ResetsAt(reset)
        );
    }

    #[test]
    fn ignores_history_before_reset_change() {
        let base = DateTime::UNIX_EPOCH;
        let old_reset = base + chrono::Duration::hours(1);
        let new_reset = base + chrono::Duration::hours(6);
        let history = vec![
            sample(base, 2., Some(old_reset)),
            sample(base + chrono::Duration::minutes(10), 98., Some(new_reset)),
            sample(base + chrono::Duration::minutes(20), 97., Some(new_reset)),
            sample(base + chrono::Duration::minutes(30), 96., Some(new_reset)),
        ];
        let usage = CodexRateLimitUsage {
            current: history.last().cloned(),
            history,
            last_error: None,
            is_stale: false,
        };

        assert_eq!(
            estimate_codex_rate_limit_projection(&usage, base + chrono::Duration::minutes(30)),
            CodexRateLimitProjection::ResetsAt(new_reset)
        );
    }
}
