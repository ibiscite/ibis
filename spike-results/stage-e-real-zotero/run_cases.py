import json, os, subprocess, sys, time, datetime

case, label, runs, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
QUERIES = ["signals", "星図", "signlas", "星図", "signals"]
env = dict(os.environ, IBIS_SPIKE_CASE=case, IBIS_SPIKE_FIXTURE="fixture", IBIS_SPIKE_RECORDS="10000",
           IBIS_SPIKE_SEED="1534", IBIS_SPIKE_PORT="23199")
results = []
for i in range(runs):
    env["IBIS_SPIKE_FIRST_QUERY"] = QUERIES[i % len(QUERIES)]
    began = time.monotonic()
    p = subprocess.run(["./bin/storage_spike", "--exact", "storage_spike_measurement", "--ignored", "--nocapture",
                        "--test-threads=1"], env=env, capture_output=True, text=True, timeout=180)
    wall = (time.monotonic() - began) * 1000
    line = [l for l in p.stdout.splitlines() if "STORAGE_SPIKE " in l]
    rec = json.loads(line[0].split("STORAGE_SPIKE ", 1)[1]) if line else None
    results.append({"label": label, "run": i, "returncode": p.returncode, "wall_ms": wall, "report": rec,
                    "stderr": p.stderr[-400:] if p.returncode else ""})
    print(label, i, p.returncode, rec and round(rec["first_results_ms"], 1), rec and round(rec["verified_ready_ms"], 1),
          rec and {k: round(v / 1e6, 1) for k, v in rec["stages_ns"].items()}, flush=True)
with open(out, "a", encoding="utf-8") as f:
    f.write(json.dumps({"utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "runs": results}) + "\n")
