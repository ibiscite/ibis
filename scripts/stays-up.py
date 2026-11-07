"""Start an installed app and fail if it exits within 20 seconds.

Preview's install job (scripts/install-and-start.sh) runs it for every package.
"""

import subprocess
import sys
import time

app = subprocess.Popen(sys.argv[1:])
time.sleep(20)
if app.poll() is not None:
    sys.exit(f"::error::{sys.argv[1]} exited with code {app.returncode} during startup")
app.terminate()
try:
    app.wait(timeout=10)
except subprocess.TimeoutExpired:
    app.kill()
print(f"{sys.argv[1]} started and stayed up")
