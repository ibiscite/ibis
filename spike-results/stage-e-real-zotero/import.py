"""Load generated items into the throwaway Zotero through its connector endpoint."""
import json, sys, time, urllib.request, urllib.error

BASE = "http://127.0.0.1:23199"
rows = [json.loads(line) for line in open("fixture/api.jsonl", encoding="utf-8")]
limit = int(sys.argv[1])
start_at = int(sys.argv[2]) if len(sys.argv) > 2 else 0


def item(row):
    data = {k: v for k, v in row["data"].items() if k not in ("key", "version", "dateModified")}
    data["attachments"] = []
    data["tags"] = [t["tag"] for t in data.get("tags", [])]
    return data


began = time.time()
for offset in range(start_at, min(limit, len(rows)), 100):
    batch = [item(r) for r in rows[offset : min(offset + 100, limit)]]
    body = json.dumps({"sessionID": f"spike-{offset}", "items": batch}).encode()
    request = urllib.request.Request(
        BASE + "/connector/saveItems",
        data=body,
        method="POST",
        headers={"Content-Type": "application/json", "X-Zotero-Connector-API-Version": "3"},
    )
    try:
        with urllib.request.urlopen(request, timeout=300) as response:
            status = response.status
    except urllib.error.HTTPError as error:
        print("HTTP", error.code, error.read()[:300], flush=True)
        raise SystemExit(1)
    if offset % 1000 == 0:
        print(offset, status, round(time.time() - began, 1), flush=True)
print("done", round(time.time() - began, 1), flush=True)
