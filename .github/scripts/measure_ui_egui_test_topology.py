#!/usr/bin/env python3
"""Compare linked UI integration-test layouts on a clean GitHub runner."""

from __future__ import annotations

import json
import os
import re
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path
from typing import BinaryIO

import psutil


ROOT = Path(__file__).resolve().parents[2]
TEMP_ROOT = Path(os.environ["RUNNER_TEMP"])
BASELINE_SHA = os.environ.get("BASELINE_SHA")
CANDIDATE_SHA = os.environ["CANDIDATE_SHA"]
TEST_RUNS = int(os.environ.get("TEST_RUNS", "3"))
TEST_COMMAND = ["cargo", "test", "-p", "pdfcraft-ui-egui", "--tests", "--locked"]
BUILD_COMMAND = [
    "cargo",
    "test",
    "-p",
    "pdfcraft-ui-egui",
    "--tests",
    "--no-run",
    "--locked",
    "--message-format=json",
]
RESULT_RE = re.compile(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored")
SAMPLE_INTERVAL_SECONDS = 0.1
REGRESSION_LIMIT_PERCENT = 5.0


def process_tree_rss(pid: int) -> int:
    try:
        root = psutil.Process(pid)
        processes = [root, *root.children(recursive=True)]
    except (psutil.NoSuchProcess, psutil.AccessDenied):
        return 0

    total = 0
    for process in processes:
        try:
            total += process.memory_info().rss
        except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
            pass
    return total


def tail_text(file: BinaryIO, limit: int = 16000) -> str:
    file.seek(0, os.SEEK_END)
    size = file.tell()
    file.seek(max(0, size - limit))
    return file.read().decode("utf-8", errors="replace")


def run_measured(command: list[str], env: dict[str, str]) -> tuple[float, int, BinaryIO, BinaryIO]:
    stdout = tempfile.TemporaryFile()
    stderr = tempfile.TemporaryFile()
    started = time.perf_counter()
    process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr)
    peak_rss = 0

    while process.poll() is None:
        peak_rss = max(peak_rss, process_tree_rss(process.pid))
        time.sleep(SAMPLE_INTERVAL_SECONDS)

    exit_code = process.wait()
    elapsed = time.perf_counter() - started
    if exit_code:
        details = (
            f"Command failed ({exit_code}): {' '.join(command)}\n"
            f"stdout tail:\n{tail_text(stdout)}\nstderr tail:\n{tail_text(stderr)}"
        )
        stdout.close()
        stderr.close()
        raise RuntimeError(details)
    return elapsed, peak_rss, stdout, stderr


def package_test_target_count(env: dict[str, str]) -> int:
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"],
        cwd=ROOT,
        env=env,
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(result.stdout)
    package = next(package for package in metadata["packages"] if package["name"] == "pdfcraft-ui-egui")
    return sum("test" in target["kind"] for target in package["targets"])


def linked_artifacts(stdout: BinaryIO) -> tuple[list[Path], int, int, int]:
    stdout.seek(0)
    executables: list[Path] = []
    for raw_line in stdout:
        try:
            message = json.loads(raw_line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") != "compiler-artifact":
            continue
        package_name = message.get("package_id", "").rsplit("#", 1)[-1].split("@", 1)[0]
        target = message.get("target", {})
        if package_name != "pdfcraft-ui-egui" or "test" not in target.get("kind", []):
            continue
        executable = message.get("executable")
        if executable:
            executables.append(Path(executable))

    if not executables:
        raise RuntimeError("Cargo emitted no pdfcraft-ui-egui integration-test executables")

    executable_bytes = sum(path.stat().st_size for path in executables)
    pdb_paths = [path.with_suffix(".pdb") for path in executables if path.with_suffix(".pdb").is_file()]
    pdb_bytes = sum(path.stat().st_size for path in pdb_paths)
    return executables, executable_bytes, pdb_bytes, len(pdb_paths)


def test_result_counts(stdout: tempfile.TemporaryFile) -> tuple[int, int, int]:
    stdout.seek(0)
    text = stdout.read().decode("utf-8", errors="replace")
    results = [tuple(map(int, match)) for match in RESULT_RE.findall(text)]
    if not results:
        raise RuntimeError("Cargo test output contained no test result summaries")
    return tuple(sum(result[index] for result in results) for index in range(3))


def measure_layout(label: str, revision: str) -> dict[str, object]:
    subprocess.run(["git", "checkout", "--detach", revision], cwd=ROOT, check=True, capture_output=True, text=True)
    if subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, check=True, capture_output=True, text=True).stdout:
        raise RuntimeError(f"Checkout {revision} left a dirty worktree")

    target_dir = TEMP_ROOT / f"ui-egui-target-{label}-{os.getpid()}"
    if target_dir.exists():
        raise RuntimeError(f"Target directory is not fresh: {target_dir}")
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target_dir)
    env["CARGO_TERM_COLOR"] = "never"
    env.pop("RUST_TEST_THREADS", None)

    try:
        expected_targets = package_test_target_count(env)
        build_seconds, build_peak_rss, build_stdout, build_stderr = run_measured(BUILD_COMMAND, env)
        try:
            executables, executable_bytes, pdb_bytes, pdb_count = linked_artifacts(build_stdout)
        finally:
            build_stdout.close()
            build_stderr.close()

        if len(executables) != expected_targets:
            raise RuntimeError(
                f"Cargo metadata lists {expected_targets} integration targets but linked "
                f"{len(executables)} executables at {revision}"
            )
        if os.name == "nt" and pdb_count != len(executables):
            raise RuntimeError(f"Found {pdb_count} Windows PDBs for {len(executables)} linked test executables")

        test_samples: list[dict[str, object]] = []
        for _ in range(TEST_RUNS):
            seconds, peak_rss, stdout, stderr = run_measured(TEST_COMMAND, env)
            try:
                passed, failed, ignored = test_result_counts(stdout)
            finally:
                stdout.close()
                stderr.close()
            if failed:
                raise RuntimeError(f"A successful Cargo test command reported {failed} failed tests")
            test_samples.append({"seconds": seconds, "peak_rss": peak_rss, "passed": passed, "ignored": ignored})

        test_counts = {(sample["passed"], sample["ignored"]) for sample in test_samples}
        if len(test_counts) != 1:
            raise RuntimeError(f"Repeated test runs discovered different test counts: {sorted(test_counts)}")

        return {
            "label": label,
            "revision": revision,
            "target_count": len(executables),
            "test_count": sum(test_counts.pop()),
            "executable_bytes": executable_bytes,
            "pdb_bytes": pdb_bytes,
            "pdb_count": pdb_count,
            "build_seconds": build_seconds,
            "build_peak_rss": build_peak_rss,
            "test_samples": test_samples,
        }
    finally:
        if target_dir.exists():
            shutil.rmtree(target_dir)


def mib(value: float) -> str:
    return f"{value / (1024 * 1024):.1f}"


def seconds_range(samples: list[dict[str, object]]) -> str:
    values = [float(sample["seconds"]) for sample in samples]
    return f"{statistics.median(values):.2f} ({min(values):.2f}–{max(values):.2f})"


def rss_range(samples: list[dict[str, object]]) -> str:
    values = [int(sample["peak_rss"]) for sample in samples]
    return f"{mib(statistics.median(values))} ({mib(min(values))}–{mib(max(values))})"


def percent_change(baseline: float, candidate: float) -> float:
    return (candidate / baseline - 1) * 100 if baseline else 0.0


def render_report(results: list[dict[str, object]]) -> str:
    baseline, candidate = results
    rows = [
        "| Layout | Revision | Test targets | Tests/run | Executable bytes | Windows PDB bytes/files | No-run wall / peak RSS | Test wall median (range) | Test peak RSS median (range) |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for result in results:
        samples = result["test_samples"]
        rows.append(
            f"| {result['label']} | `{str(result['revision'])[:12]}` | {result['target_count']} | {result['test_count']} | "
            f"{result['executable_bytes']:,} | {result['pdb_bytes']:,} / {result['pdb_count']} | "
            f"{float(result['build_seconds']):.2f} s / {mib(float(result['build_peak_rss']))} MiB | "
            f"{seconds_range(samples)} s | {rss_range(samples)} MiB |"
        )

    baseline_samples = baseline["test_samples"]
    candidate_samples = candidate["test_samples"]
    baseline_test_wall = statistics.median(float(sample["seconds"]) for sample in baseline_samples)
    candidate_test_wall = statistics.median(float(sample["seconds"]) for sample in candidate_samples)
    baseline_test_rss = statistics.median(int(sample["peak_rss"]) for sample in baseline_samples)
    candidate_test_rss = statistics.median(int(sample["peak_rss"]) for sample in candidate_samples)
    build_wall_change = percent_change(float(baseline["build_seconds"]), float(candidate["build_seconds"]))
    build_rss_change = percent_change(float(baseline["build_peak_rss"]), float(candidate["build_peak_rss"]))
    test_wall_change = percent_change(baseline_test_wall, candidate_test_wall)
    test_rss_change = percent_change(baseline_test_rss, candidate_test_rss)
    baseline_linked_bytes = int(baseline["executable_bytes"]) + int(baseline["pdb_bytes"])
    candidate_linked_bytes = int(candidate["executable_bytes"]) + int(candidate["pdb_bytes"])
    linked_change = percent_change(baseline_linked_bytes, candidate_linked_bytes)
    material_regression = max(build_rss_change, test_wall_change, test_rss_change) > REGRESSION_LIMIT_PERCENT
    improvement = candidate_linked_bytes < baseline_linked_bytes or build_wall_change < 0

    rows.extend(
        [
            "",
            f"Build wall change: {build_wall_change:+.1f}%; build peak process-tree RSS change: {build_rss_change:+.1f}%.",
            f"Repeated test median wall change: {test_wall_change:+.1f}%; median peak process-tree RSS change: {test_rss_change:+.1f}%.",
            f"Aggregate linked executable/PDB bytes change: {linked_change:+.1f}%.",
            f"Retention gate (5% maximum build-RSS/test-time/test-RSS regression): {'PASS' if improvement and not material_regression else 'REJECT'}.",
            "Measurements use a fresh `CARGO_TARGET_DIR` per layout and three default-parallel `cargo test -p pdfcraft-ui-egui --tests --locked` runs.",
        ]
    )
    if int(baseline["test_count"]) != int(candidate["test_count"]):
        rows.append(f"Test-count parity: FAIL ({baseline['test_count']} vs {candidate['test_count']}).")
    else:
        rows.append(f"Test-count parity: PASS ({candidate['test_count']} tests).")
    return "\n".join(rows) + "\n"


def main() -> None:
    subprocess.run(["cargo", "fetch", "--locked"], cwd=ROOT, check=True)
    baseline_sha = BASELINE_SHA or subprocess.run(
        ["git", "rev-parse", f"{CANDIDATE_SHA}^"], cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout.strip()
    results = [measure_layout("baseline", baseline_sha), measure_layout("candidate", CANDIDATE_SHA)]
    report = render_report(results)
    report_path = TEMP_ROOT / "ui-egui-topology-measurement.md"
    report_path.write_text(report, encoding="utf-8")
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        with open(summary_path, "a", encoding="utf-8") as summary:
            summary.write("## UI egui test topology\n\n")
            summary.write(report)
    print(report, end="")

    baseline, candidate = results
    if int(baseline["test_count"]) != int(candidate["test_count"]):
        raise SystemExit("Baseline and candidate discovered different numbers of tests")
    if not int(candidate["target_count"]) < int(baseline["target_count"]):
        raise SystemExit("Candidate did not reduce integration-test executable count")
    if "REJECT" in report:
        raise SystemExit("Candidate failed the linked-size/time or 5% regression retention gate")


if __name__ == "__main__":
    main()
