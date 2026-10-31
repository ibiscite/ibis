"""Stage A, fresh release processes, five retained runs per source and size.

Use python3. All generated data/builds are under
target; only the JSON evidence is under spike-results. Never discovers a real
profile, launches Zotero, changes production files, installs or uses a network.
The optional stub binds loopback only. A denied socket/process check is retained
as environmental failure, never replaced with a cheaper reader.
"""

import argparse
import ctypes
import datetime
import json
import math
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import parse_qs, urlparse


SUPPORT = Path(__file__).with_suffix("")
sys.path.insert(0, str(SUPPORT))
from fixtures import ASSUMPTIONS, ROOT, digest, generate  # noqa: E402
from instrument import build  # noqa: E402

CASES = ["bib-cache", "bib-none", "zotero-lean", "zotero-fulltext", "zotero-live"]
ARGS = ["--exact", "storage_spike_measurement", "--ignored", "--nocapture", "--test-threads=1"]
METRICS = ["spawn_to_first_results_ms", "verified_ready_ms", "peak_process_mib", "settled_process_mib"]
STAGES = ["read_copy", "quick_check", "parse", "normalize", "snapshot_fingerprints", "index", "hydrate"]


class Memory:
    def __init__(self):
        self.lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True) if sys.platform == "darwin" else None

    def mib(self, pid):
        if self.lib:
            buffer = ctypes.create_string_buffer(96)
            count = self.lib.proc_pidinfo(pid, 4, 0, buffer, len(buffer))  # PROC_PIDTASKINFO
            if count != 96:
                raise OSError(ctypes.get_errno(), "proc_pidinfo failed")
            return ctypes.cast(buffer, ctypes.POINTER(ctypes.c_uint64))[1] / 1024**2
        fields = Path(f"/proc/{pid}/statm").read_text().split()
        return int(fields[1]) * os.sysconf("SC_PAGE_SIZE") / 1024**2


class Stub:
    """All API JSON and batches preloaded before timing; zero artificial delay."""

    def __init__(self, fixture):
        rows = [json.loads(line) for line in (fixture / "api.jsonl").read_text().splitlines()]
        keyed = {row["key"]: row for row in rows}
        inventory = json.dumps(dict.fromkeys(keyed, 1), separators=(",", ":")).encode()
        ordered = sorted(keyed)
        batches = {",".join(keys): json.dumps([keyed[key] for key in keys], ensure_ascii=False, separators=(",", ":")).encode()
                   for offset in range(0, len(rows), 50) if (keys := ordered[offset:offset + 50])}

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                query = parse_qs(urlparse(self.path).query)
                if self.path.startswith("/api/users/0/items/top?") and query.get("format") == ["versions"]:
                    body, total = inventory, len(rows)
                elif query.get("itemKey", [None])[0] in batches:
                    body, total = batches[query["itemKey"][0]], len(query["itemKey"][0].split(","))
                else:
                    self.send_error(404)
                    return
                self.send_response(200)
                for key, value in {"Content-Length": len(body), "Connection": "close", "Zotero-API-Version": 3,
                                   "X-Zotero-Version": "10.0.6", "Zotero-Server-ID": "invented-stage-a",
                                   "Last-Modified-Version": 1, "Total-Results": total}.items():
                    self.send_header(key, str(value))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_):
                pass

        self.server = HTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    @property
    def port(self):
        return self.server.server_port

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)


def measure(binary, env, memory):
    samples, stdout, stderr, first = [], [], [], []
    start = time.monotonic()
    child = subprocess.Popen([str(binary), *ARGS], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    def read_output():
        for line in child.stdout:
            if "STORAGE_FIRST " in line:
                first.append((time.monotonic() - start) * 1000)
            stdout.append(line)

    def read_errors():
        stderr.append(child.stderr.read())

    readers = [threading.Thread(target=read_output), threading.Thread(target=read_errors)]
    for reader in readers:
        reader.start()
    failure = None
    usage = None
    while True:
        pid, status, current_usage = os.wait4(child.pid, os.WNOHANG)
        if pid:
            child.returncode = os.waitstatus_to_exitcode(status)
            usage = current_usage
            break
        try:
            samples.append([time.monotonic() - start, memory.mib(child.pid)])
        except OSError as error:
            if error.errno != 3:  # ESRCH: exited between wait4 and sampling
                failure = f"memory sampling unavailable: {error}"
        if time.monotonic() - start > 180:
            child.kill()
            failure = "process exceeded 180 seconds"
        time.sleep(0.02)
    for reader in readers:
        reader.join(timeout=5)
    child.stdout.close()
    child.stderr.close()
    result = {"returncode": child.returncode, "stdout": "".join(stdout), "stderr": "".join(stderr),
              "rss_samples": samples, "wall_ms": (time.monotonic() - start) * 1000}
    if failure:
        result["sampling_error"] = failure
    reports = [line.split("STORAGE_SPIKE ", 1)[1] for line in stdout if line.startswith("STORAGE_SPIKE ")]
    if len(reports) == 1:
        result.update(json.loads(reports[0]))
    if len(first) == 1:
        result["spawn_to_first_results_ms"] = first[0]
    if usage:
        result["peak_process_mib"] = usage.ru_maxrss / (1024**2 if sys.platform == "darwin" else 1024)
    if len(samples) >= 10:
        result["peak_process_mib"] = max(result.get("peak_process_mib", 0), max(value for _, value in samples))
        result["settled_process_mib"] = statistics.median(value for _, value in samples[-10:])
    return result


def validate(run):
    reasons = []
    if run.get("returncode") or not run.get("release"):
        reasons.append("failed or non-release process")
    if run.get("record_count") != run["records"] or run.get("first_hits") != 50:
        reasons.append("wrong/missing record or first-hit count")
    if run.get("seed") != 1534:
        reasons.append("stale/missing fixture seed")
    for metric in METRICS:
        value = run.get(metric, 0)
        if not isinstance(value, (float, int)) or not math.isfinite(value) or value <= 0:
            reasons.append(f"missing/invalid {metric}")
    if run.get("sampling_error"):
        reasons.append(run["sampling_error"])
    if not {"index", "snapshot_fingerprints"} <= set(run.get("stages_ns", {})):
        reasons.append("missing stage probes")
    if len(run.get("queries", [])) != 3 or any(query.get("hits") != 50 for query in run.get("queries", [])):
        reasons.append("wrong/missing query evidence")
    return reasons


def distribution(values):
    return {"median": statistics.median(values), "min": min(values), "max": max(values)}


def machine_info(free):
    result = {"system": platform.system(), "release": platform.release(), "architecture": platform.machine(),
              "python": platform.python_version(), "cpu_count": os.cpu_count(), "free_bytes_before": free}
    if sys.platform == "darwin":
        lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib", use_errno=True)
        for name in ["hw.model", "machdep.cpu.brand_string", "hw.memsize"]:
            size = ctypes.c_size_t(0)
            if lib.sysctlbyname(name.encode(), None, ctypes.byref(size), None, 0) == 0:
                buffer = ctypes.create_string_buffer(size.value)
                if lib.sysctlbyname(name.encode(), buffer, ctypes.byref(size), None, 0) == 0:
                    result[name] = int.from_bytes(buffer.raw, "little") if name == "hw.memsize" else buffer.value.decode()
    return result


def summarize(report, ratio):
    for records in [10000, 50000]:
        baseline = [run for run in report["runs"] if run["records"] == records and run["case"] == "bib-cache"]
        rejects = []
        if len(baseline) != 5 or any(run["rejections"] for run in baseline):
            rejects.append("baseline missing/failed")
        else:
            for metric in METRICS:
                values = [run[metric] for run in baseline]
                if max(values) / min(values) > ratio:
                    rejects.append(f"unexplained baseline excursion: {metric}, ratio={max(values) / min(values)}")
        report["baseline_validity"][str(records)] = {"valid": not rejects, "rejections": rejects}
        for case in CASES:
            runs = [run for run in report["runs"] if run["records"] == records and run["case"] == case]
            accepted = len(runs) == 5 and all(not run["rejections"] for run in runs)
            summary = {"case": case, "records": records, "valid_processes": accepted,
                       "valid_comparison": accepted and not rejects}
            if accepted:
                summary["metrics"] = {metric: distribution([run[metric] for run in runs]) for metric in [*METRICS, "first_results_ms", "first_search_ms", "loaded_ms"]}
                summary["stage_ms"] = {stage: distribution([run["stages_ns"].get(stage, 0) / 1e6 for run in runs]) for stage in STAGES}
                summary["stage_ms"]["other"] = distribution([run["first_results_ms"] - run["first_search_ms"] - sum(run["stages_ns"].values()) / 1e6 for run in runs])
                summary["queries_ms"] = {query: distribution([run["queries"][i]["ms"] for run in runs]) for i, query in enumerate(["signals", "星図", "signlas"])}
                summary["worst_margin_to_1s_ms"] = 1000 - summary["metrics"]["spawn_to_first_results_ms"]["max"]
            report["summaries"].append(summary)
    report["complete"] = len(report["runs"]) == 50 and all(item["valid_comparison"] for item in report["summaries"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "spike-results/stage-a.json")
    parser.add_argument("--reuse-fixtures", action="store_true")
    parser.add_argument("--baseline-max-ratio", type=float, default=1.25)
    parser.add_argument("--prior-attempt", type=Path, help="retain a diagnosed invalid harness attempt in this report")
    args = parser.parse_args()
    if not math.isfinite(args.baseline_max_ratio) or args.baseline_max_ratio <= 1:
        parser.error("baseline ratio must be finite and >1")
    free = shutil.disk_usage(ROOT).free
    if free < 10_000_000_000:
        raise RuntimeError("less than 10 GB free; stopping")
    os.environ["CARGO_TARGET_DIR"] = str(ROOT / "target")
    os.environ["CARGO_NET_OFFLINE"] = "true"
    order = []
    for round_ in range(5):
        for records in ([10000, 50000] if round_ % 2 == 0 else [50000, 10000]):
            rotation = CASES[round_:] + CASES[:round_]
            for case in rotation if records == 10000 else reversed(rotation):
                order.append({"round": round_ + 1, "records": records, "case": case,
                              "first_query": ["signals", "星図", "signlas", "星図", "signals"][round_]})
    report = {"stage": "A", "seed": 1534, "created_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "machine": machine_info(free),
              "assumptions": ASSUMPTIONS, "protocol": {
                  "order": order, "runs_per_case": 5, "baseline_max_min_ratio": args.baseline_max_ratio,
                  "validity": "all baseline metrics max/min <= ratio separately at each size; no discarded runs or silent retries; every process must pass count, seed, warning-free fixture, citation and three-query oracles",
                  "timing": "spawn-to-first stdout marker includes process startup and manifest read; engine first-results starts immediately before Engine/cache creation; verified-ready also includes oracle validation and one checked key citation",
                  "stages": "exclusive nested Instant spans in disposable copy of real reader code; normalize=CSL mapping, index includes search-text normalization; residual includes native checks, schema bounds, cleanup, publication, ownership and probe overhead",
                  "memory": "20 ms process RSS samples (macOS proc_pidinfo / Linux statm); kernel wait4 peak RSS; settled median last 10 samples during 1 s hold; engine process only, excludes external API stub",
                  "scope": "excludes app launch, WebView, cold OS file cache (no purge), other machines, fixture generation/seeding, compilation, live Zotero work/latency and closed profile discovery/live-API fallback probe",
                  "fulltext": "DB bytes are allocated table/index pages and invented postings; not sparse-file padding; 600 MB decimal assumption"},
              "fixtures": [], "seeds": [], "runs": [], "baseline_validity": {}, "summaries": [], "environment": [], "complete": False}
    if args.prior_attempt:
        report["prior_attempts"] = [{"reason": "diagnosed harness failure: libtest prefixes the first output line, so launch marker was missed; transient ESRCH at process exit was misclassified as sampling failure; no prior run discarded", "report": json.loads(args.prior_attempt.read_text())}]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as output:
        json.dump(report, output, indent=2)

    def save():
        args.output.write_text(json.dumps(report, indent=2) + "\n")

    binary, provenance = build()
    report["build"] = provenance
    save()
    root = ROOT / "target/stage-a-fixtures"
    root.mkdir(exist_ok=True)
    environments, stubs = {}, {}
    memory = Memory()
    try:
        for count in [10000, 50000]:
            fixture = root / str(count)
            if fixture.exists() and not args.reuse_fixtures:
                raise RuntimeError("fixture exists; use --reuse-fixtures (hashes are revalidated)")
            manifest = json.loads((fixture / "manifest.json").read_text()) if fixture.exists() else generate(fixture, count, 1534, True)
            assert manifest["seed"] == 1534 and manifest["records"] == count
            for path, evidence in manifest["files"].items():
                assert digest(fixture / path) == evidence["sha256"], "stale or malformed fixture hash"
            report["fixtures"].append(manifest)
            env = dict(os.environ, IBIS_SPIKE_FIXTURE=str(fixture), IBIS_SPIKE_RECORDS=str(count), IBIS_SPIKE_SEED="1534")
            # Existing cache seed is not trusted; reseed through the normal path.
            if (fixture / "cache").exists():
                shutil.rmtree(fixture / "cache")
            seeded = subprocess.run([str(binary), *ARGS], env=dict(env, IBIS_SPIKE_CASE="seed"), capture_output=True, text=True, timeout=180)
            report["seeds"].append({"records": count, "returncode": seeded.returncode, "stdout": seeded.stdout, "stderr": seeded.stderr})
            if seeded.returncode:
                save()
                raise RuntimeError("cache seeding failed; no error path may be timed")
            environments[count] = env
            try:
                stubs[count] = Stub(fixture)
                env["IBIS_SPIKE_PORT"] = str(stubs[count].port)
            except OSError as error:
                report["environment"].append({"case": "zotero-live", "records": count, "error": str(error), "meaning": "loopback stub cannot listen; real live reader attempts retained as environmental failures"})
                env["IBIS_SPIKE_PORT"] = "0"  # guaranteed invalid fixture port; process must fail, never counted as timing
            save()
        for entry in order:
            print(f"round {entry['round']} {entry['records']} {entry['case']}", flush=True)
            run = dict(entry)
            run.update(measure(binary, dict(environments[entry["records"]], IBIS_SPIKE_CASE=entry["case"], IBIS_SPIKE_FIRST_QUERY=entry["first_query"]), memory))
            run["rejections"] = validate(run)
            report["runs"].append(run)
            save()
        summarize(report, args.baseline_max_ratio)
        save()
        print(json.dumps({"complete": report["complete"], "baseline_validity": report["baseline_validity"]}))
        return 0 if report["complete"] else 1
    finally:
        for stub in stubs.values():
            stub.close()


if __name__ == "__main__":
    sys.exit(main())
