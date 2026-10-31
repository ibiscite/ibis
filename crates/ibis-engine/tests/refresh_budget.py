"""Release open/refresh gate, or fresh-process cached verification comparison.

--cached BASELINE CANDIDATE --output REPORT compares release file_cache test
executables containing cached_verification_measurement. Seed outside measurement;
retain every run. Engine-only results do not qualify native app/webview budgets.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[3]
METRICS = ("cached_first_search_ms", "verified_ready_ms", "peak_tree_mib", "settled_tree_mib")
CASE = ["--exact", "cached_verification_measurement", "--ignored", "--nocapture", "--test-threads=1"]


def tree_mib(pid):
    rows = subprocess.run(
        ["ps", "-axo", "pid=,ppid=,rss="], capture_output=True, text=True, check=True, timeout=2
    ).stdout.splitlines()
    processes = [tuple(map(int, row.split())) for row in rows]
    owned = {pid}
    while True:
        children = {child for child, parent, _ in processes if parent in owned}
        if children <= owned:
            return sum(rss for child, _, rss in processes if child in owned) / 1024
        owned |= children


def measure(binary, env):
    samples = []
    start = time.monotonic()
    with subprocess.Popen([str(binary), *CASE], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as child:
        failure = None
        try:
            while child.poll() is None:
                if time.monotonic() - start > 180:
                    raise TimeoutError("measurement exceeded 180 seconds")
                rss = tree_mib(child.pid)
                if rss:
                    samples.append([time.monotonic() - start, rss])
                time.sleep(0.02)
        except (OSError, ValueError, subprocess.SubprocessError, TimeoutError) as error:
            failure = str(error)
            child.kill()
        stdout, stderr = child.communicate()
    result = {"returncode": child.returncode, "stdout": stdout, "stderr": stderr, "rss_samples": samples}
    if failure:
        result["error"] = failure
    elif child.returncode == 0 and len(samples) >= 10:
        reports = [line.split("CACHE_BUDGET ", 1)[1] for line in stdout.splitlines() if "CACHE_BUDGET " in line]
        if len(reports) == 1:
            result.update(json.loads(reports[0]))
            result["peak_tree_mib"] = max(rss for _, rss in samples)
            result["settled_tree_mib"] = statistics.median(rss for _, rss in samples[-10:])
    return result


def cached_comparison(arguments):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", nargs=2, type=Path, metavar="BASELINE/CANDIDATE")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--baseline-max-ratio", type=float, default=1.25)
    args = parser.parse_args(arguments)
    if not 1 < args.baseline_max_ratio < float("inf"):
        parser.error("baseline max/min ratio must be finite and greater than one")
    binaries = dict(zip(("baseline", "candidate"), (path.resolve(strict=True) for path in args.binaries)))
    order = [
        {"records": records, "label": label}
        for records, reverse in [(10_000, False), (100_000, True)]
        for pair in range(5)
        for label in (("candidate", "baseline") if (pair % 2 == 1) != reverse else ("baseline", "candidate"))
    ]
    report = {
        "executable_sha256": {label: hashlib.sha256(binary.read_bytes()).hexdigest() for label, binary in binaries.items()},
        "protocol": {"order": order, "baseline_max_min_ratio": args.baseline_max_ratio,
                     "validity": "all four baseline metrics must meet the predeclared max/min ratio; no discarded runs",
                     "rss": "20 ms process-tree sampling; settled median of last 10 samples during 1 s hold",
                     "scope": "engine only; excludes fixture generation, seeding, compilation and native webviews"},
        "seeds": [], "runs": [], "rejections": [], "valid": False,
    }
    # Refuse to erase earlier evidence, then persist after every attempted process.
    with args.output.open("x") as output:
        json.dump(report, output, indent=2)

    def save():
        args.output.write_text(json.dumps(report, indent=2) + "\n")

    (ROOT / "target").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="cache-verification-", dir=ROOT / "target") as directory:
        directory = Path(directory)
        environments = {}
        for records in (10_000, 100_000):
            source = directory / f"synthetic-{records}.bib"
            with source.open("w") as fixture:
                for index in range(records):
                    fixture.write(
                        f"@article{{synthetic{index},title={{Synthetic signals {index}}},"
                        f"author={{Researcher{index % 5000}, Avery and Morgan{index % 701}, Ann}},"
                        f"year={{{1950 + index % 75}}},journal={{Invented Letters}},pages={{1--20}},"
                        f"doi={{10.1000/synthetic{index}}},volume={{{index % 12 + 1}}},number={{{index % 4 + 1}}}}}\n"
                    )
            for label, binary in binaries.items():
                env = dict(os.environ, IBIS_CACHE_SOURCE=str(source), IBIS_CACHE_STORE=str(directory / f"{label}-{records}"),
                           IBIS_CACHE_RECORDS=str(records), IBIS_CACHE_MODE="seed")
                seed = {"label": label, "records": records}
                try:
                    seeded = subprocess.run([str(binary), *CASE], env=env, capture_output=True, text=True, timeout=180)
                    seed.update(returncode=seeded.returncode, stdout=seeded.stdout, stderr=seeded.stderr)
                except (OSError, subprocess.SubprocessError) as error:
                    seed.update(returncode=-1, error=str(error))
                report["seeds"].append(seed)
                environments[label, records] = dict(env, IBIS_CACHE_MODE="measure")
                save()
        for entry in order:
            run = dict(entry)
            run.update(measure(binaries[entry["label"]], environments[entry["label"], entry["records"]]))
            report["runs"].append(run)
            save()
    if any(seed["returncode"] for seed in report["seeds"]):
        report["rejections"].append("cache seeding failed")
    for run in report["runs"]:
        if run["returncode"] or not run.get("release") or any(not run.get(metric, 0) > 0 for metric in METRICS):
            report["rejections"].append(f"incomplete/non-release run: {run['label']} {run['records']}")
    if not report["rejections"]:
        for records in (10_000, 100_000):
            for metric in METRICS:
                baseline = [run[metric] for run in report["runs"] if run["label"] == "baseline" and run["records"] == records]
                if max(baseline) / min(baseline) > args.baseline_max_ratio:
                    report["rejections"].append(f"unexplained baseline excursion: {records} {metric}")
    report["valid"] = not report["rejections"]
    if report["valid"]:
        report["medians"] = [
            dict(label=label, records=records, **{metric: statistics.median(
                run[metric] for run in report["runs"] if run["label"] == label and run["records"] == records
            ) for metric in METRICS})
            for records in (10_000, 100_000) for label in binaries
        ]
    save()
    print(json.dumps({"valid": report["valid"], "rejections": report["rejections"]}))
    return 0 if report["valid"] else 1


def open_refresh(binary):
    import resource

    result = subprocess.run(
        [binary, "open_refresh_budget", "--ignored", "--nocapture", "--test-threads=1"], check=False
    )
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    divisor = 1024 * 1024 if sys.platform == "darwin" else 1024
    print(f"100k process: peak-rss={usage.ru_maxrss / divisor:.1f}MiB", flush=True)
    return result.returncode


def main():
    if sys.argv[1:2] == ["--cached"]:
        return cached_comparison(sys.argv[2:])
    if len(sys.argv) == 2:
        return open_refresh(sys.argv[1])
    build = subprocess.run(
        [
            "cargo", "test", "-p", "ibis-engine", "--locked", "--release",
            "--test", "refresh_budget", "--no-run", "--message-format=json",
        ],
        cwd=ROOT,
        env=dict(os.environ, CARGO_TARGET_DIR=os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target"))),
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    artifacts = [json.loads(line) for line in build.stdout.splitlines()]
    binary = next(
        item["executable"] for item in artifacts
        if item.get("reason") == "compiler-artifact"
        and item["target"]["name"] == "refresh_budget"
        and item.get("executable")
    )
    return subprocess.run([sys.executable, __file__, binary], check=False).returncode


if __name__ == "__main__":
    sys.exit(main())
