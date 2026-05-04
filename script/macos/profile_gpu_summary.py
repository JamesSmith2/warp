#!/usr/bin/env python3

import argparse
import json
import re
import sys
import xml.etree.ElementTree as ET
from collections import Counter, defaultdict
from pathlib import Path


def cell_text(elem):
    return "".join(elem.itertext()).strip()


def load_rows(path):
    path = Path(path)
    if not path.exists() or path.stat().st_size == 0:
        return []

    root = ET.parse(path).getroot()
    id_values = {}
    for elem in root.iter():
        ident = elem.attrib.get("id")
        if ident is not None:
            id_values[ident] = {
                "fmt": elem.attrib.get("fmt", ""),
                "raw": cell_text(elem),
            }

    def value_for(elem):
        ref = elem.attrib.get("ref")
        if ref is not None:
            value = id_values.get(ref, {})
            return value.get("fmt") or value.get("raw") or ""
        return elem.attrib.get("fmt") or cell_text(elem)

    def raw_for(elem):
        ref = elem.attrib.get("ref")
        if ref is not None:
            value = id_values.get(ref, {})
            return value.get("raw") or value.get("fmt") or ""
        return cell_text(elem) or elem.attrib.get("fmt", "")

    rows = []
    for node in root.findall(".//node"):
        schema = node.find("schema")
        if schema is None:
            continue

        columns = [
            col.findtext("mnemonic") or f"column_{index}"
            for index, col in enumerate(schema.findall("col"))
        ]
        for row in node.findall("row"):
            values = {}
            for column, elem in zip(columns, list(row)):
                values[column] = value_for(elem)
                values[f"{column}__raw"] = raw_for(elem)
            rows.append(values)
    return rows


def integer_value(value):
    if value is None:
        return None
    text = str(value).replace("\xa0", "").replace(",", "").strip()
    if not text:
        return None
    if re.fullmatch(r"-?\d+", text):
        return int(text)
    match = re.search(r"-?\d+", text)
    if match is None:
        return None
    return int(match.group(0))


def numeric(row, column):
    return integer_value(row.get(f"{column}__raw")) or integer_value(row.get(column))


def process_matches(row, target_re, target_pid):
    searchable = " ".join(
        str(row.get(column, ""))
        for column in ("process", "thread", "event-label", "track-label")
    )
    if target_pid and (
        f"({target_pid})" in searchable or f"pid={target_pid}" in searchable
    ):
        return True
    if target_pid:
        return False
    if target_re.search(searchable):
        return True
    return False


def matched_processes(rows, target_re, target_pid):
    return [
        row
        for row in rows
        if process_matches(row, target_re=target_re, target_pid=target_pid)
    ]


def union_duration_ns(intervals):
    merged = []
    for start, end in sorted(intervals):
        if start is None or end is None or end <= start:
            continue
        if not merged or start > merged[-1][1]:
            merged.append([start, end])
        else:
            merged[-1][1] = max(merged[-1][1], end)
    return sum(end - start for start, end in merged)


def percentile(values, percentile_value):
    if not values:
        return 0.0
    ordered = sorted(values)
    index = int((percentile_value / 100.0) * len(ordered) + 0.999999) - 1
    index = max(0, min(index, len(ordered) - 1))
    return ordered[index]


def bucket_busy_percentages(intervals, trace_duration_ns, bucket_ns):
    if trace_duration_ns <= 0 or bucket_ns <= 0:
        return []

    bucket_count = max(1, (trace_duration_ns + bucket_ns - 1) // bucket_ns)
    bucket_intervals = [[] for _ in range(bucket_count)]
    for start, end in intervals:
        if start is None or end is None or end <= start:
            continue
        start = max(0, start)
        end = min(trace_duration_ns, end)
        if end <= start:
            continue

        first_bucket = start // bucket_ns
        last_bucket = (end - 1) // bucket_ns
        for bucket_index in range(first_bucket, last_bucket + 1):
            bucket_start = bucket_index * bucket_ns
            bucket_end = min(trace_duration_ns, bucket_start + bucket_ns)
            overlap_start = max(start, bucket_start)
            overlap_end = min(end, bucket_end)
            if overlap_end > overlap_start:
                bucket_intervals[bucket_index].append((overlap_start, overlap_end))

    percentages = []
    for bucket_index, intervals_for_bucket in enumerate(bucket_intervals):
        bucket_start = bucket_index * bucket_ns
        bucket_end = min(trace_duration_ns, bucket_start + bucket_ns)
        bucket_duration = bucket_end - bucket_start
        percentages.append(percent(union_duration_ns(intervals_for_bucket), bucket_duration))
    return percentages


def rate(count, duration_seconds):
    if duration_seconds <= 0:
        return 0.0
    return count / duration_seconds


def summarize_gpu_intervals(rows, duration_seconds, target_re, target_pid, bucket_ms):
    target_rows = matched_processes(rows, target_re, target_pid)
    active_rows = [
        row
        for row in target_rows
        if not row.get("state") or row.get("state") == "Active"
    ]

    intervals = []
    durations = []
    by_channel = defaultdict(lambda: {"count": 0, "duration_ns": 0, "max_ns": 0})
    for row in active_rows:
        start = numeric(row, "start")
        duration = numeric(row, "duration")
        if start is None or duration is None:
            continue
        durations.append(duration)
        intervals.append((start, start + duration))
        channel = row.get("channel-name") or "unknown"
        by_channel[channel]["count"] += 1
        by_channel[channel]["duration_ns"] += duration
        by_channel[channel]["max_ns"] = max(by_channel[channel]["max_ns"], duration)

    trace_duration_ns = int(duration_seconds * 1_000_000_000)
    total_ns = sum(durations)
    merged_ns = union_duration_ns(intervals)
    processes = Counter(row.get("process", "") for row in target_rows if row.get("process"))
    bucket_percentages = bucket_busy_percentages(
        intervals, trace_duration_ns, int(bucket_ms * 1_000_000)
    )

    return {
        "matched_rows": len(target_rows),
        "active_interval_count": len(active_rows),
        "processes": dict(processes.most_common()),
        "duration_sum_ns": total_ns,
        "duration_union_ns": merged_ns,
        "raw_busy_percent": percent(total_ns, trace_duration_ns),
        "merged_busy_percent": percent(merged_ns, trace_duration_ns),
        "max_interval_us": round((max(durations) if durations else 0) / 1000.0, 3),
        "avg_interval_us": round((sum(durations) / len(durations) if durations else 0) / 1000.0, 3),
        "bucket_ms": bucket_ms,
        "bucket_count": len(bucket_percentages),
        "bucket_busy_percent_p50": round(percentile(bucket_percentages, 50), 3),
        "bucket_busy_percent_p95": round(percentile(bucket_percentages, 95), 3),
        "bucket_busy_percent_p99": round(percentile(bucket_percentages, 99), 3),
        "bucket_busy_percent_max": round(max(bucket_percentages) if bucket_percentages else 0.0, 3),
        "by_channel": {
            channel: {
                "count": values["count"],
                "duration_ns": values["duration_ns"],
                "raw_busy_percent": percent(values["duration_ns"], trace_duration_ns),
                "max_us": round(values["max_ns"] / 1000.0, 3),
            }
            for channel, values in sorted(by_channel.items())
        },
    }


def summarize_count_table(rows, duration_seconds, target_re, target_pid, duration_column=None):
    target_rows = matched_processes(rows, target_re, target_pid)
    durations = []
    for row in target_rows:
        if duration_column:
            value = numeric(row, duration_column)
            if value is not None:
                durations.append(value)
    command_buffers = {
        row.get("cmdbuffer-id")
        for row in target_rows
        if row.get("cmdbuffer-id")
    }
    return {
        "count": len(target_rows),
        "per_second": round(rate(len(target_rows), duration_seconds), 3),
        "unique_command_buffers": len(command_buffers),
        "duration_sum_ns": sum(durations),
        "max_duration_us": round((max(durations) if durations else 0) / 1000.0, 3),
    }


def percent(numerator, denominator):
    if denominator <= 0:
        return 0.0
    return round(100.0 * numerator / denominator, 3)


def diagnostics_tail(path):
    if path is None:
        return []
    path = Path(path)
    if not path.exists():
        return []
    lines = [
        line.rstrip()
        for line in path.read_text(errors="replace").splitlines()
        if "render diagnostics window=" in line
    ]
    return lines[-20:]


def parse_debug_map(text):
    text = text.strip()
    if text == "{}":
        return {}

    result = {}
    for match in re.finditer(r'(?:"([^"]+)"|([A-Za-z_][A-Za-z0-9_]*)): (\d+)', text):
        key = match.group(1) or match.group(2)
        result[key] = result.get(key, 0) + int(match.group(3))
    return result


def diagnostics_summary(path):
    if path is None:
        return {}
    path = Path(path)
    if not path.exists():
        return {}

    counters = Counter()
    durations_us = Counter()
    repaint_sources = Counter()
    line_count = 0

    for line in path.read_text(errors="replace").splitlines():
        if "render diagnostics window=" not in line:
            continue
        line_count += 1

        counters_match = re.search(r"counters=(\{.*?\}) durations_us=", line)
        if counters_match:
            counters.update(parse_debug_map(counters_match.group(1)))

        durations_match = re.search(r"durations_us=(\{.*?\}) repaint_sources=", line)
        if durations_match:
            durations_us.update(parse_debug_map(durations_match.group(1)))

        repaint_match = re.search(r"repaint_sources=(\{.*\})$", line)
        if repaint_match:
            repaint_sources.update(parse_debug_map(repaint_match.group(1)))

    return {
        "line_count": line_count,
        "counters": dict(counters.most_common()),
        "durations_us": dict(durations_us.most_common()),
        "repaint_sources": dict(repaint_sources.most_common()),
    }


def print_summary(summary):
    print(f"run_dir: {summary['run_dir']}")
    print(f"trace_duration_s: {summary['trace_duration_s']}")
    print(f"target_regex: {summary['target_regex']}")
    if summary.get("target_pid"):
        print(f"target_pid: {summary['target_pid']}")

    gpu = summary["gpu_intervals"]
    print("metal_gpu_intervals:")
    print(f"  matched_rows: {gpu['matched_rows']}")
    print(f"  active_interval_count: {gpu['active_interval_count']}")
    print(f"  merged_busy_percent: {gpu['merged_busy_percent']}")
    print(f"  raw_busy_percent: {gpu['raw_busy_percent']}")
    print(f"  bucket_ms: {gpu['bucket_ms']}")
    print(f"  bucket_busy_percent_p99: {gpu['bucket_busy_percent_p99']}")
    print(f"  bucket_busy_percent_max: {gpu['bucket_busy_percent_max']}")
    print(f"  max_interval_us: {gpu['max_interval_us']}")
    print(f"  avg_interval_us: {gpu['avg_interval_us']}")
    if gpu["processes"]:
        print(f"  processes: {gpu['processes']}")
    if gpu["by_channel"]:
        print("  by_channel:")
        for channel, values in gpu["by_channel"].items():
            print(
                "    "
                + channel
                + ": "
                + json.dumps(values, sort_keys=True, separators=(",", ":"))
            )

    for name in ("command_buffers", "present_requests", "buffer_waits"):
        values = summary[name]
        print(f"{name}:")
        print(f"  count: {values['count']}")
        print(f"  per_second: {values['per_second']}")
        print(f"  unique_command_buffers: {values['unique_command_buffers']}")
        print(f"  duration_sum_ns: {values['duration_sum_ns']}")
        print(f"  max_duration_us: {values['max_duration_us']}")

    if summary["diagnostics_tail"]:
        diagnostics = summary.get("diagnostics_summary") or {}
        if diagnostics:
            print("render_diagnostics_summary:")
            print(f"  line_count: {diagnostics.get('line_count', 0)}")
            if diagnostics.get("counters"):
                print("  counters:")
                for name, count in diagnostics["counters"].items():
                    print(f"    {name}: {count}")
            if diagnostics.get("repaint_sources"):
                print("  repaint_sources:")
                for name, count in diagnostics["repaint_sources"].items():
                    print(f"    {name}: {count}")

        print("render_diagnostics_tail:")
        for line in summary["diagnostics_tail"]:
            print(f"  {line}")

    if summary["threshold_failures"]:
        print("threshold_failures:")
        for failure in summary["threshold_failures"]:
            print(f"  {failure}")

    if summary.get("baseline_delta"):
        print("baseline_delta:")
        print(json.dumps(summary["baseline_delta"], indent=2, sort_keys=True))


def add_baseline_delta(summary, baseline, args):
    gpu = summary["gpu_intervals"]
    baseline_gpu = baseline["gpu_intervals"]

    delta = {
        "baseline_run_dir": baseline.get("run_dir"),
        "merged_busy_percent_delta": round(
            gpu["merged_busy_percent"] - baseline_gpu["merged_busy_percent"], 3
        ),
        "bucket_busy_percent_p99_delta": round(
            gpu["bucket_busy_percent_p99"] - baseline_gpu["bucket_busy_percent_p99"], 3
        ),
        "present_requests_per_second_delta": round(
            summary["present_requests"]["per_second"]
            - baseline["present_requests"]["per_second"],
            3,
        ),
        "command_buffers_per_second_delta": round(
            summary["command_buffers"]["per_second"]
            - baseline["command_buffers"]["per_second"],
            3,
        ),
    }
    summary["baseline_delta"] = delta

    if delta["merged_busy_percent_delta"] > args.max_baseline_gpu_busy_delta:
        summary["threshold_failures"].append(
            f"merged Metal GPU busy delta {delta['merged_busy_percent_delta']}pp > {args.max_baseline_gpu_busy_delta}pp baseline allowance"
        )
    if delta["bucket_busy_percent_p99_delta"] > args.max_baseline_p99_busy_delta:
        summary["threshold_failures"].append(
            f"p99 bucket Metal GPU busy delta {delta['bucket_busy_percent_p99_delta']}pp > {args.max_baseline_p99_busy_delta}pp baseline allowance"
        )
    if delta["present_requests_per_second_delta"] > args.max_baseline_presents_delta:
        summary["threshold_failures"].append(
            f"present request delta {delta['present_requests_per_second_delta']}/s > {args.max_baseline_presents_delta}/s baseline allowance"
        )


def main():
    parser = argparse.ArgumentParser(description="Summarize Warp OSS Metal trace exports.")
    parser.add_argument("--run-dir", required=True)
    parser.add_argument("--duration-seconds", required=True, type=float)
    parser.add_argument("--target-re", default=r"(WarpOss|warp-oss)")
    parser.add_argument("--target-pid")
    parser.add_argument("--gpu-intervals", required=True)
    parser.add_argument("--command-buffers", required=True)
    parser.add_argument("--present-requests", required=True)
    parser.add_argument("--buffer-waits", required=True)
    parser.add_argument("--diagnostics-log")
    parser.add_argument("--json-out", required=True)
    parser.add_argument("--max-gpu-busy-percent", type=float, default=10.0)
    parser.add_argument("--max-gpu-p99-busy-percent", type=float, default=10.0)
    parser.add_argument("--max-command-buffers-per-second", type=float, default=120.0)
    parser.add_argument("--max-presents-per-second", type=float, default=120.0)
    parser.add_argument("--max-buffer-waits", type=int, default=10)
    parser.add_argument("--baseline-json")
    parser.add_argument("--max-baseline-gpu-busy-delta", type=float, default=2.0)
    parser.add_argument("--max-baseline-p99-busy-delta", type=float, default=2.0)
    parser.add_argument("--max-baseline-presents-delta", type=float, default=2.0)
    parser.add_argument("--bucket-ms", type=int, default=1000)
    parser.add_argument("--fail-thresholds", action="store_true")
    args = parser.parse_args()

    target_re = re.compile(args.target_re)
    gpu_rows = load_rows(args.gpu_intervals)
    command_rows = load_rows(args.command_buffers)
    present_rows = load_rows(args.present_requests)
    wait_rows = load_rows(args.buffer_waits)

    summary = {
        "run_dir": args.run_dir,
        "trace_duration_s": args.duration_seconds,
        "target_regex": args.target_re,
        "target_pid": args.target_pid,
        "gpu_intervals": summarize_gpu_intervals(
            gpu_rows, args.duration_seconds, target_re, args.target_pid, args.bucket_ms
        ),
        "command_buffers": summarize_count_table(
            command_rows, args.duration_seconds, target_re, args.target_pid
        ),
        "present_requests": summarize_count_table(
            present_rows, args.duration_seconds, target_re, args.target_pid
        ),
        "buffer_waits": summarize_count_table(
            wait_rows,
            args.duration_seconds,
            target_re,
            args.target_pid,
            duration_column="duration",
        ),
        "diagnostics_summary": diagnostics_summary(args.diagnostics_log),
        "diagnostics_tail": diagnostics_tail(args.diagnostics_log),
        "threshold_failures": [],
    }

    if summary["gpu_intervals"]["merged_busy_percent"] > args.max_gpu_busy_percent:
        summary["threshold_failures"].append(
            f"merged Metal GPU busy {summary['gpu_intervals']['merged_busy_percent']}% > {args.max_gpu_busy_percent}%"
        )
    if summary["gpu_intervals"]["bucket_busy_percent_p99"] > args.max_gpu_p99_busy_percent:
        summary["threshold_failures"].append(
            f"p99 bucket Metal GPU busy {summary['gpu_intervals']['bucket_busy_percent_p99']}% > {args.max_gpu_p99_busy_percent}%"
        )
    if summary["command_buffers"]["per_second"] > args.max_command_buffers_per_second:
        summary["threshold_failures"].append(
            f"command buffers {summary['command_buffers']['per_second']}/s > {args.max_command_buffers_per_second}/s"
        )
    if summary["present_requests"]["per_second"] > args.max_presents_per_second:
        summary["threshold_failures"].append(
            f"present requests {summary['present_requests']['per_second']}/s > {args.max_presents_per_second}/s"
        )
    if summary["buffer_waits"]["count"] > args.max_buffer_waits:
        summary["threshold_failures"].append(
            f"CA buffer waits {summary['buffer_waits']['count']} > {args.max_buffer_waits}"
        )

    if args.baseline_json:
        baseline = json.loads(Path(args.baseline_json).read_text())
        add_baseline_delta(summary, baseline, args)

    Path(args.json_out).write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print_summary(summary)

    if args.fail_thresholds and summary["threshold_failures"]:
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
