"""Run the release budget gate and report its peak RSS (macOS/Linux).

Uses only Python's standard library. Compilation runs in a separate process so
the compiler's peak is excluded. Run from any directory; honors CARGO_TARGET_DIR.
"""

import json
from pathlib import Path
import subprocess
import sys


if len(sys.argv) == 2:
    import resource

    result = subprocess.run(
        [sys.argv[1], "open_refresh_budget", "--ignored", "--nocapture", "--test-threads=1"], check=False
    )
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    divisor = 1024 * 1024 if sys.platform == "darwin" else 1024
    print(f"100k process: peak-rss={usage.ru_maxrss / divisor:.1f}MiB", flush=True)
    sys.exit(result.returncode)

build = subprocess.run(
    [
        "cargo", "test", "-p", "ibis-engine", "--locked", "--release",
        "--test", "refresh_budget", "--no-run", "--message-format=json",
    ],
    cwd=Path(__file__).resolve().parents[3],
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
sys.exit(subprocess.run([sys.executable, __file__, binary], check=False).returncode)
