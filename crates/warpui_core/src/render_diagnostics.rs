use std::{
    collections::HashMap,
    env,
    fs::{File, OpenOptions},
    io::Write,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde::Serialize;

use crate::WindowId;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderDiagnosticEvent {
    RequestRedraw,
    DisplayAlreadyPending,
    SetNeedsDisplayAsync,
    RenderScene,
    SubmitScene,
    RequestFrameCapture,
    RedrawRequested,
    SceneBuild,
    MacUpdateLayer,
    NextDrawable,
    CommandBufferCommit,
    CommandBufferWait,
    GpuActive,
    Present,
    TerminalSurfaceSubmitted,
    TerminalSurfaceRows,
    TerminalSurfaceDirtyRows,
    TerminalSurfaceFullFallback,
    TerminalWakeup,
    TerminalWakeupRendered,
    TerminalWakeupDeferred,
    TerminalWakeupThrottled,
    TerminalWakeupHidden,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct WindowRenderDiagnostics {
    pub counters: HashMap<RenderDiagnosticEvent, u64>,
    pub durations_us: HashMap<RenderDiagnosticEvent, u64>,
    pub repaint_sources: HashMap<&'static str, u64>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct RenderDiagnosticsSnapshot {
    pub windows: HashMap<WindowId, WindowRenderDiagnostics>,
}

#[derive(Debug, Default)]
struct RenderDiagnosticsState {
    snapshot: RenderDiagnosticsSnapshot,
    last_log: Option<RenderDiagnosticsLogState>,
}

#[derive(Debug)]
struct RenderDiagnosticsLogState {
    instant: Instant,
    snapshot: RenderDiagnosticsSnapshot,
}

static STATE: OnceLock<Mutex<RenderDiagnosticsState>> = OnceLock::new();
static ENABLED: OnceLock<bool> = OnceLock::new();
static LOG_DELTAS: OnceLock<bool> = OnceLock::new();
static DELTA_FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();
static FORCE_ENABLED: AtomicBool = AtomicBool::new(false);
static TOTAL_GPU_ACTIVE_US: AtomicU64 = AtomicU64::new(0);

pub fn enabled() -> bool {
    if FORCE_ENABLED.load(Ordering::Relaxed) {
        return true;
    }

    *ENABLED.get_or_init(|| env::var_os("WARP_RENDER_DIAGNOSTICS").is_some())
}

fn should_log_deltas() -> bool {
    *LOG_DELTAS.get_or_init(|| env::var_os("WARP_RENDER_DIAGNOSTICS_LOG").is_some())
}

fn delta_file() -> Option<&'static Mutex<File>> {
    DELTA_FILE
        .get_or_init(|| {
            env::var_os("WARP_RENDER_DIAGNOSTICS_FILE").and_then(|path| {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .ok()
                    .map(Mutex::new)
            })
        })
        .as_ref()
}

pub fn record_event(window_id: WindowId, event: RenderDiagnosticEvent) {
    record_count(window_id, event, 1);
}

pub fn record_count(window_id: WindowId, event: RenderDiagnosticEvent, count: u64) {
    if !enabled() {
        return;
    }

    let mut state = STATE
        .get_or_init(|| Mutex::new(RenderDiagnosticsState::default()))
        .lock()
        .expect("render diagnostics mutex poisoned");
    let window = state.snapshot.windows.entry(window_id).or_default();
    *window.counters.entry(event).or_default() += count;
    maybe_log_deltas(&mut state);
}

pub fn record_duration(window_id: WindowId, event: RenderDiagnosticEvent, duration: Duration) {
    if !enabled() {
        return;
    }

    let mut state = STATE
        .get_or_init(|| Mutex::new(RenderDiagnosticsState::default()))
        .lock()
        .expect("render diagnostics mutex poisoned");
    let window = state.snapshot.windows.entry(window_id).or_default();
    *window.counters.entry(event).or_default() += 1;
    *window.durations_us.entry(event).or_default() += duration.as_micros() as u64;
    maybe_log_deltas(&mut state);
}

pub fn record_gpu_active_duration(window_id: WindowId, duration: Duration) {
    let duration_us = duration.as_micros().min(u128::from(u64::MAX)) as u64;
    if duration_us == 0 {
        return;
    }

    TOTAL_GPU_ACTIVE_US.fetch_add(duration_us, Ordering::Relaxed);
    record_duration(window_id, RenderDiagnosticEvent::GpuActive, duration);
}

pub fn total_gpu_active_us() -> u64 {
    TOTAL_GPU_ACTIVE_US.load(Ordering::Relaxed)
}

pub fn record_repaint_source(window_id: WindowId, source: &'static str) {
    if !enabled() {
        return;
    }

    let mut state = STATE
        .get_or_init(|| Mutex::new(RenderDiagnosticsState::default()))
        .lock()
        .expect("render diagnostics mutex poisoned");
    let window = state.snapshot.windows.entry(window_id).or_default();
    *window.repaint_sources.entry(source).or_default() += 1;
    maybe_log_deltas(&mut state);
}

pub fn snapshot() -> RenderDiagnosticsSnapshot {
    STATE
        .get_or_init(|| Mutex::new(RenderDiagnosticsState::default()))
        .lock()
        .expect("render diagnostics mutex poisoned")
        .snapshot
        .clone()
}

pub fn reset() {
    if let Some(state) = STATE.get() {
        *state.lock().expect("render diagnostics mutex poisoned") =
            RenderDiagnosticsState::default();
    }
    TOTAL_GPU_ACTIVE_US.store(0, Ordering::Relaxed);
}

fn maybe_log_deltas(state: &mut RenderDiagnosticsState) {
    if !should_log_deltas() {
        return;
    }

    let now = Instant::now();
    let Some(last_log) = &state.last_log else {
        state.last_log = Some(RenderDiagnosticsLogState {
            instant: now,
            snapshot: state.snapshot.clone(),
        });
        return;
    };

    if now.duration_since(last_log.instant) < Duration::from_secs(1) {
        return;
    }

    let elapsed = now.duration_since(last_log.instant).as_secs_f64();
    for (window_id, window) in &state.snapshot.windows {
        let previous = last_log.snapshot.windows.get(window_id);
        let counter_deltas = window
            .counters
            .iter()
            .filter_map(|(event, value)| {
                let previous_value = previous
                    .and_then(|window| window.counters.get(event))
                    .copied()
                    .unwrap_or_default();
                let delta = value.saturating_sub(previous_value);
                (delta > 0).then_some((*event, delta))
            })
            .collect::<HashMap<_, _>>();
        let duration_deltas_us = window
            .durations_us
            .iter()
            .filter_map(|(event, value)| {
                let previous_value = previous
                    .and_then(|window| window.durations_us.get(event))
                    .copied()
                    .unwrap_or_default();
                let delta = value.saturating_sub(previous_value);
                (delta > 0).then_some((*event, delta))
            })
            .collect::<HashMap<_, _>>();
        let repaint_source_deltas = window
            .repaint_sources
            .iter()
            .filter_map(|(source, value)| {
                let previous_value = previous
                    .and_then(|window| window.repaint_sources.get(source))
                    .copied()
                    .unwrap_or_default();
                let delta = value.saturating_sub(previous_value);
                (delta > 0).then_some((*source, delta))
            })
            .collect::<HashMap<_, _>>();

        if counter_deltas.is_empty()
            && duration_deltas_us.is_empty()
            && repaint_source_deltas.is_empty()
        {
            continue;
        }

        log::info!(
            "render diagnostics window={window_id:?} elapsed_s={elapsed:.2} counters={counter_deltas:?} durations_us={duration_deltas_us:?} repaint_sources={repaint_source_deltas:?}"
        );
        if let Some(file) = delta_file() {
            let _ = writeln!(
                file.lock().expect("render diagnostics file mutex poisoned"),
                "render diagnostics window={window_id:?} elapsed_s={elapsed:.2} counters={counter_deltas:?} durations_us={duration_deltas_us:?} repaint_sources={repaint_source_deltas:?}"
            );
        }
    }

    state.last_log = Some(RenderDiagnosticsLogState {
        instant: now,
        snapshot: state.snapshot.clone(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_window_events_and_repaint_sources() {
        FORCE_ENABLED.store(true, Ordering::Relaxed);
        reset();

        let window_id = WindowId::new();
        record_event(window_id, RenderDiagnosticEvent::RequestRedraw);
        record_event(window_id, RenderDiagnosticEvent::RequestRedraw);
        record_count(window_id, RenderDiagnosticEvent::TerminalSurfaceRows, 17);
        record_duration(
            window_id,
            RenderDiagnosticEvent::CommandBufferWait,
            Duration::from_micros(25),
        );
        record_gpu_active_duration(window_id, Duration::from_micros(125));
        record_repaint_source(window_id, "test_source");

        let snapshot = snapshot();
        let window = snapshot
            .windows
            .get(&window_id)
            .expect("window diagnostics should be recorded");

        assert_eq!(
            window.counters.get(&RenderDiagnosticEvent::RequestRedraw),
            Some(&2)
        );
        assert_eq!(
            window
                .counters
                .get(&RenderDiagnosticEvent::TerminalSurfaceRows),
            Some(&17)
        );
        assert_eq!(
            window
                .durations_us
                .get(&RenderDiagnosticEvent::CommandBufferWait),
            Some(&25)
        );
        assert_eq!(
            window.durations_us.get(&RenderDiagnosticEvent::GpuActive),
            Some(&125)
        );
        assert_eq!(total_gpu_active_us(), 125);
        assert_eq!(window.repaint_sources.get("test_source"), Some(&1));

        reset();
        FORCE_ENABLED.store(false, Ordering::Relaxed);
    }
}
