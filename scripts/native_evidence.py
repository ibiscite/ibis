"""Bounded, allowlisted native evidence. No desktop values or exceptions are serialized.

The tape is a replay guard around the real scenario, never a source of commands.
Only relative order is replayed; application IPC is outside this adapter's view.
"""
from contextlib import contextmanager
from contextvars import ContextVar
from functools import cache
import hashlib
import json
import locale
import math
import os
from pathlib import Path
import platform
import re
import subprocess
import time
import uuid

SCHEMA = Path(__file__).with_name("native_evidence.schema.json")
MAX_BYTES = 1024 * 1024
ACTIVE = ContextVar("native_evidence", default=None)
REPLAY_COMMAND = "python3 scripts/smoke-native.py --app BUILD --output REPLAY --replay BUNDLE/manifest.json"
KEYS = {value: "key-" + code for value, code in (
    ("Escape", "escape"), ("Return", "return"), ("Tab", "tab"), ("space", "space"),
    ("Down", "down"), ("ctrl+a", "select-all"), ("ctrl+k", "menu"),
    ("ctrl+shift+y", "shortcut"))}
TEXT = {value: "text-" + code for value, code in (
    ("smoke000000", "query-0"), ("smoke000001", "query-1"), ("smoke000002", "query-2"),
    ("smoke000003", "query-3"), ("smoke099999", "query-last"), ("good2", "damaged"),
    ("zzqx no such reference", "no-match"), ("Typst", "typst"),
    ("ibis tab probe", "probe"), ("ibis tab probe alternate", "probe-alternate"))}
CLICKS = {value: "click-" + code for value, code in (
    ("More", "more"), ("Library", "library"), ("Reload Library", "reload"), ("Quit Ibis", "quit"),
    ("Copy as Pandoc key", "format"))}


class BundleError(RuntimeError):
    """Messages are fixed codes/sentences, never untrusted JSON or OS errors."""


@cache
def _schema():
    return json.loads(SCHEMA.read_text(encoding="utf-8"))


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def _matches(value, schema, root):
    """The small JSON Schema subset used by our checked-in, closed schema."""
    if "$ref" in schema:
        return _matches(value, root["$defs"][schema["$ref"].split("/")[-1]], root)
    if "anyOf" in schema:
        return any(_matches(value, alternative, root) for alternative in schema["anyOf"])
    if "const" in schema and (type(value) is not type(schema["const"]) or value != schema["const"]):
        return False
    if "enum" in schema and not any(type(value) is type(item) and value == item for item in schema["enum"]):
        return False
    kind = schema.get("type")
    kinds = {"object": dict, "array": list, "string": str, "integer": int, "null": type(None), "boolean": bool}
    if kind == "number":
        if type(value) not in (int, float) or (type(value) is float and not math.isfinite(value)):
            return False
    elif kind and type(value) is not kinds[kind]:
        return False
    if type(value) is dict:
        properties = schema.get("properties", {})
        if not set(schema.get("required", ())) <= value.keys():
            return False
        if schema.get("additionalProperties") is False and not value.keys() <= properties.keys():
            return False
        if not all(_matches(item, properties.get(key, schema.get("additionalProperties", {})), root)
                   for key, item in value.items()):
            return False
        if "propertyNames" in schema and not all(_matches(key, schema["propertyNames"], root) for key in value):
            return False
    if type(value) is list:
        if not schema.get("minItems", 0) <= len(value) <= schema.get("maxItems", MAX_BYTES):
            return False
        if schema.get("uniqueItems") and len(set(map(json.dumps, value))) != len(value):
            return False
        if not all(_matches(item, schema["items"], root) for item in value):
            return False
    if type(value) is str:
        if len(value) > schema.get("maxLength", MAX_BYTES):
            return False
        if "pattern" in schema and not re.fullmatch(schema["pattern"], value, flags=re.ASCII):
            return False
    if type(value) in (int, float):
        if not schema.get("minimum", -math.inf) <= value <= schema.get("maximum", math.inf):
            return False
    return True


def validate_bundle(bundle):
    if type(bundle) is not dict or type(bundle.get("schema_version")) is not int or bundle["schema_version"] != 1:
        raise BundleError("Unsupported evidence schema version; expected 1")
    schema = _schema()
    if not _matches(bundle, schema, schema):
        raise BundleError("Malformed evidence bundle; schema validation failed")
    pending, next_id, completions, last_time = {}, 1, [], 0
    action_codes = schema["$defs"]["action"]["enum"]
    for event in bundle["events"]:
        identity, phase = event["id"], event["phase"]
        if event["at_ms"] < last_time or (event["kind"] == "action") != (event["code"] in action_codes):
            raise BundleError("Malformed evidence event order")
        last_time = event["at_ms"]
        signature = (event["kind"], event["code"])
        if phase == "dispatch":
            if identity != next_id:
                raise BundleError("Malformed evidence request identity")
            next_id += 1
            pending[identity] = signature
        else:
            if pending.pop(identity, None) != signature:
                raise BundleError("Malformed evidence completion order")
            if event["kind"] == "request":
                completions.append(identity)
    if pending or completions != bundle["request_completion_order"]:
        raise BundleError("Malformed evidence completion order")
    result = bundle["result"]
    outcome = result["outcome"]
    if ((outcome == "pass" and any(result[key] is not None for key in ("failed_invariant", "reason", "failure_kind")))
            or (outcome != "pass" and result["reason"] is None)
            or (outcome == "fail" and (result["failed_invariant"] is None or result["failure_kind"] is None))
            or (outcome == "unavailable" and (not bundle["missing_capabilities"] or result["failed_invariant"] is not None
                                              or result["failure_kind"] is not None
                                              or result["reason"] != "native-capability-unavailable"))
            or (outcome == "fail" and result["reason"] == "native-capability-unavailable")
            or (outcome != "unavailable" and bundle["missing_capabilities"])):
        raise BundleError("Malformed evidence result classification")
    environment = bundle["environment"]
    if set(environment["unavailable"]) != {key for key, value in environment.items() if value is None}:
        raise BundleError("Malformed evidence environment availability")
    return bundle


def load_bundle(path):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError()
            result[key] = value
        return result
    def invalid(_value):
        raise ValueError()
    try:
        with Path(path).open("rb") as source:
            content = source.read(MAX_BYTES + 1)
        if len(content) > MAX_BYTES:
            raise ValueError()
        bundle = json.loads(content.decode("utf-8"), object_pairs_hook=unique, parse_constant=invalid)
        return validate_bundle(bundle)
    except (OSError, ValueError, UnicodeError, RecursionError):
        raise BundleError("Malformed or unreadable evidence bundle (limit: 1 MiB)") from None


class Evidence:
    def __init__(self, scenario, options, metadata, replay=None):
        self.started = time.monotonic()
        self.bundle = dict(metadata, schema_version=1, run_id=uuid.uuid4().hex,
                           scenario=scenario, options=options, seed_reason="deterministic-scenario",
                           events=[], request_completion_order=[], missing_capabilities=[],
                           app_request_order={"value": None, "reason": "not-observable"},
                           environment_unavailable_reason="not-observed",
                           replay_command=REPLAY_COMMAND, replay_of=replay["run_id"] if replay else None)
        self.replay = replay
        self.pending = {}
        self.next_id = 1
        self.failed_invariant = None
        self.failure_kind = "harness"
        self.failure_reason = "harness-failed"
        self.missing_capability = "native-session"
        self.diverged = False
        if replay:
            validate_bundle(replay)
            # Refuse a different build, fixture, harness, scenario or observed
            # environment before any desktop action. Paths are never compared.
            for key in ("scenario", "options", "build", "harness_sha256", "fixtures", "environment"):
                if self.bundle[key] != replay[key]:
                    raise BundleError("Replay inputs differ; use the recorded build, harness, fixtures and environment")

    def _event(self, identity, kind, code, phase):
        event = {"id": identity, "kind": kind, "code": code, "phase": phase,
                 "at_ms": max(0, round((time.monotonic() - self.started) * 1000, 3))}
        events = self.bundle["events"]
        if len(events) >= 4000:
            raise BundleError("Evidence event limit exceeded")
        if self.replay and not self.diverged:
            index = len(events)
            expected = self.replay["events"]
            if index >= len(expected) or any(event[key] != expected[index][key] for key in ("id", "kind", "code", "phase")):
                self.diverged = True
                raise BundleError("Replay sequence diverged; native action refused")
        events.append(event)

    def dispatch(self, kind, code):
        if self.diverged:
            raise BundleError("Replay sequence diverged; native action refused")
        # Validate codes before side effects, including when not replaying.
        schema = _schema()
        definition = schema["$defs"]["action" if kind == "action" else "request"]
        if kind not in ("action", "request") or not _matches(code, definition, schema):
            raise BundleError("Unrecordable native action refused")
        if len(self.bundle["events"]) + len(self.pending) + 2 > 4000:
            raise BundleError("Evidence event limit exceeded")
        identity = self.next_id
        self._event(identity, kind, code, "dispatch")
        self.pending[identity] = (kind, code)
        self.next_id += 1
        return identity

    def complete(self, identity, phase):
        kind, code = self.pending[identity]
        divergence = None
        try:
            self._event(identity, kind, code, phase)
        except BundleError as error:
            # The completion already happened. Retain the actual outcome even
            # when its order differs from replay; never repeat the native call.
            if not self.diverged:
                raise
            self._event(identity, kind, code, phase)
            divergence = error
        finally:
            self.pending.pop(identity, None)
        if kind == "request":
            self.bundle["request_completion_order"].append(identity)
        if phase == "reject" and self.failed_invariant is None:
            self.failed_invariant = code
        if divergence:
            raise divergence

    @contextmanager
    def step(self, kind, code):
        previous = self.failed_invariant, self.failure_kind, self.failure_reason
        identity = self.dispatch(kind, code)
        try:
            yield
        except BaseException as error:
            self.failure(error)
            self.complete(identity, "cancel" if isinstance(error, (KeyboardInterrupt, SystemExit)) else "reject")
            raise
        else:
            self.complete(identity, "complete")
            # A successful enclosing request owns its handled retries. A later
            # failure must name its own invariant rather than a recovered one.
            self.failed_invariant, self.failure_kind, self.failure_reason = previous

    def failure(self, error):
        self.failure_kind = "product" if isinstance(error, AssertionError) else "harness"
        if isinstance(error, (KeyboardInterrupt, SystemExit)):
            self.failure_reason = "interrupted"
        elif isinstance(error, subprocess.TimeoutExpired) or (isinstance(error, AssertionError) and str(error).startswith("Timed out:")):
            self.failure_reason = "timeout"
        elif isinstance(error, OSError):
            self.failure_reason = "native-io-failed"
        else:
            self.failure_reason = "invariant-failed" if isinstance(error, AssertionError) else "harness-failed"

    def finish(self, exit_code):
        for identity in list(self.pending):
            try:
                self.complete(identity, "cancel")
            except BundleError:
                if not self.diverged:
                    raise
        outcome = {0: "pass", 2: "unavailable"}.get(exit_code, "fail")
        result = {"outcome": outcome, "failure_kind": self.failure_kind if outcome == "fail" else None,
                  "reason": {"pass": None, "fail": self.failure_reason, "unavailable": "native-capability-unavailable"}[outcome],
                  "failed_invariant": (self.failed_invariant or "scenario") if outcome == "fail" else None}
        missing = [self.missing_capability] if outcome == "unavailable" else []
        if self.replay and (self.diverged or len(self.bundle["events"]) != len(self.replay["events"])
                            or result != self.replay["result"] or missing != self.replay["missing_capabilities"]):
            result = {"outcome": "fail", "failure_kind": "harness", "reason": "replay-diverged",
                      "failed_invariant": "replay-sequence"}
        self.bundle["result"] = result
        self.bundle["missing_capabilities"] = missing if result["outcome"] == "unavailable" else []
        return validate_bundle(self.bundle)

    def write(self, output, bundle):
        validate_bundle(bundle)
        content = (json.dumps(bundle, indent=2, allow_nan=False) + "\n").encode("utf-8")
        if len(content) > MAX_BYTES:
            raise BundleError("Evidence size limit exceeded")
        folder = Path(output) / ("evidence-" + self.bundle["run_id"])
        try:
            folder.mkdir(parents=True, exist_ok=False)
            path = folder / "manifest.json"
            with path.open("xb") as target:
                target.write(content)
            return path
        except OSError:
            raise BundleError("Evidence write failed; coverage was not recorded") from None


@contextmanager
def recording(kind, code):
    recorder = ACTIVE.get()
    if recorder:
        with recorder.step(kind, code):
            yield
    else:
        yield


def failure(error, invariant=None):
    if ACTIVE.get():
        ACTIVE.get().failure(error)
        if invariant:
            ACTIVE.get().failed_invariant = invariant


def unavailable(code):
    if ACTIVE.get():
        ACTIVE.get().missing_capability = code


def observe_native(native):
    """Wrap input only; snapshots, clipboard readback and errors never enter evidence."""
    recorder = ACTIVE.get()
    if not recorder:
        return native
    keys = dict(KEYS, **{recorder.bundle["options"]["shortcut"]: "key-shortcut"})
    for method, codes in (("key", keys), ("text", TEXT), ("click", CLICKS),
                          ("set_clipboard", {"Ibis native smoke clipboard sentinel": "clipboard-sentinel"}),
                          ("focus_editor", {None: "focus-editor"})):
        if not hasattr(native, method):
            continue
        original = getattr(native, method)
        def action(value=None, *, _original=original, _codes=codes, _method=method):
            code = _codes.get(value)
            if code is None:
                raise BundleError("Unrecordable native input refused")
            with recorder.step("action", code):
                return _original() if _method == "focus_editor" else _original(value)
        setattr(native, method, action)
    return native


def _probe(*command, cwd=None):
    try:
        result = subprocess.run(command, cwd=cwd, capture_output=True, timeout=5, check=False,
                                env=dict(os.environ, GIT_OPTIONAL_LOCKS="0", GIT_NO_LAZY_FETCH="1"))
        return result.stdout if result.returncode == 0 and type(result.stdout) is bytes else b""
    except Exception:
        return b""  # Missing metadata never exposes subprocess details.


def _version(value):
    value = value.strip()
    return value if re.fullmatch(r"[0-9]+(?:[.:-][0-9]+)*(?:-0ubuntu[0-9.]+)?", value, re.ASCII) else None


def collect_metadata(app, fixtures):
    root = Path(__file__).resolve().parent.parent
    revision = _probe("git", "rev-parse", "HEAD", cwd=root).decode("ascii", errors="replace").strip()
    patch = _probe("git", "diff", "--binary", "--no-ext-diff", "--no-textconv", "HEAD", cwd=root)
    architecture = None
    try:
        digest = hashlib.sha256()
        with app.open("rb") as source:
            header = source.read(20)
            if header[:4] == b"\x7fELF" and len(header) == 20:
                endian = {1: "little", 2: "big"}.get(header[5])
                if endian:
                    architecture = {62: "x86_64", 183: "aarch64"}.get(int.from_bytes(header[18:20], endian))
            digest.update(header)
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
        artifact = digest.hexdigest()
    except OSError:
        artifact = None
    version = re.search(r'^version = "([0-9.]+)"$', (root / "Cargo.toml").read_text(), re.MULTILINE)
    harness = b"".join((root / "scripts" / name).read_bytes() for name in
                       ("smoke-native.py", "native_evidence.py", "native_evidence.schema.json", "native_test_settings.py"))
    language = locale.setlocale(locale.LC_CTYPE)
    if not re.fullmatch(r"(?:C|POSIX|[a-z]{2,3}(?:_[A-Z]{2})?)(?:\.(?:UTF-8|utf8|ISO8859-1))?", language, re.ASCII):
        language = None
    environment = {key: None for key in ("webview_version", "display_pixels", "display_mm", "display_count",
                                         "keyboard_layout", "input_method", "text_size", "theme", "contrast",
                                         "motion", "fonts", "scale", "citation_language")}
    system = platform.system()
    os_version = _version(platform.release())
    try:
        if system == "Linux":
            os_version = _version(platform.freedesktop_os_release().get("VERSION_ID", ""))
        elif system == "Darwin":
            os_version = _version(platform.mac_ver()[0])
        elif system == "Windows":
            os_version = _version(platform.win32_ver()[1])
    except OSError:
        pass
    environment.update(os={"Linux": "linux", "Darwin": "macos", "Windows": "windows"}.get(platform.system(), "other"),
                       os_version=os_version, os_build=_version(platform.release()), locale=language)
    environment["unavailable"] = sorted(key for key, value in environment.items() if value is None)
    if platform.system() == "Linux":
        observe_environment(environment)
    return {"build": {"revision": revision if re.fullmatch("[0-9a-f]{40}", revision) else None,
                      "dirty_patch_sha256": sha256(patch) if patch else None,
                      "artifact_sha256": artifact, "app_version": version[1] if version else None,
                      "architecture": architecture,
                      "profile": app.parent.name if app.parent.name in ("debug", "release") else None},
            "build_unavailable_reason": "not-observed",
            "harness_sha256": sha256(harness), "fixtures": {key: sha256(value) for key, value in fixtures.items()},
            "environment": environment}


def observe_environment(environment=None):
    recorder = ACTIVE.get()
    if environment is None and not recorder:
        return
    if environment is None:
        environment = recorder.bundle["environment"]
    dimensions = _probe("xdpyinfo").decode("ascii", errors="replace")
    match = re.search(r"dimensions:\s+(\d+)x(\d+) pixels \((\d+)x(\d+) millimeters\)", dimensions)
    if match:
        environment["display_pixels"] = [int(match[1]), int(match[2])]
        environment["display_mm"] = [int(match[3]), int(match[4])]
    monitors = _probe("xrandr", "--listmonitors").decode("ascii", errors="replace")
    match = re.search(r"\AMonitors: (\d+)\n", monitors)
    if match:
        environment["display_count"] = int(match[1])
    environment["webview_version"] = _version(_probe("dpkg-query", "-W", "-f=${Version}", "libwebkit2gtk-4.1-0").decode("ascii", errors="replace"))
    environment["unavailable"] = sorted(key for key, value in environment.items() if value is None and key != "unavailable")
    if recorder and recorder.replay and environment != recorder.replay["environment"]:
        raise BundleError("Replay environment differs; native action refused")
