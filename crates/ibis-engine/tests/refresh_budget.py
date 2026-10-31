"""Release open/refresh gate, with fresh-process peak RSS."""

import json
import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[3]


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
