#!/usr/bin/env python3
"""Observe ordinary native appearance and restore OS preferences through AX UI."""
import argparse
from contextlib import contextmanager
import json
import math
import os
from pathlib import Path
import platform
import re
import select
import subprocess
import tempfile
import time


class Unavailable(RuntimeError):
    pass


class HelperTimeout(RuntimeError):
    pass


APPEARANCE_PHASES = ("material", "reduce-transparency", "material-after-reduce-transparency",
                     "increase-contrast", "material-after-increase-contrast")


def picker_frame(value):
    if not isinstance(value, dict) or set(value) != {"window", "position", "size", "screen"}:
        raise RuntimeError("Native picker frame metadata unavailable")
    if type(value["window"]) is not int or value["window"] <= 0:
        raise RuntimeError("Native picker window identity unavailable")
    for key in ("position", "size", "screen"):
        pair = value[key]
        if not isinstance(pair, list) or len(pair) != 2 or any(type(v) not in (int, float) or not math.isfinite(v) for v in pair):
            raise RuntimeError("Native picker frame dimensions unavailable")
    x, y = value["position"]
    width, height = value["size"]
    screen_width, screen_height = value["screen"]
    if x < 0 or y < 0 or width < 360 or height < 320 or x + width > screen_width or y + height > screen_height:
        raise RuntimeError("Native picker is outside the capture screen")
    return {key: list(item) if isinstance(item, list) else item for key, item in value.items()}


def pin_picker_window(read):
    window = None
    def current():
        nonlocal window
        frame = picker_frame(read())
        if window is None:
            window = frame["window"]
        elif frame["window"] != window:
            raise RuntimeError("Native picker window identity changed")
        return frame
    return current


def owned_picker_call(process, ui, action, request):
    if action not in {"picker-owner", "picker-frame", "picker-size"}:
        raise RuntimeError("Unknown owned picker action")
    token = request if action == "picker-owner" else request.get("owner")
    if not isinstance(token, dict) or type(token.get("pid")) is not int or token["pid"] != process.pid:
        raise RuntimeError("Owned picker process identity changed")
    # This is our unreaped Popen child, not a PID lookup. Its PID cannot be
    # reused while alive or a zombie. No other path waits/reaps this child;
    # reject before invocation or after observation if it exits in between.
    if process.poll() is not None:
        raise RuntimeError("Owned native appearance process exited")
    result = ui.call(action, json.dumps(request))
    if process.poll() is not None:
        raise RuntimeError("Owned native appearance process exited")
    return result


def capture_sizes(name, output, read, resize, capture):
    if name not in APPEARANCE_PHASES:
        raise RuntimeError("Unknown native appearance capture phase")
    original = picker_frame(read())
    target = [int(original["size"][0]) + 120, int(original["size"][1]) + 80]
    picker_frame({**original, "size": target})
    if target[0] > 1280 or target[1] > 900:
        raise RuntimeError("Resized picker exceeds the capture bound")
    def matches(size):
        current = picker_frame(read())
        if current["window"] != original["window"]:
            raise RuntimeError("Native picker window identity changed")
        return all(abs(actual - wanted) <= 1 for actual, wanted in zip(current["size"], size))
    try:
        capture(output / f"{name}.png")
        resize(original["window"], target)
        wait_for(lambda: matches(target), "owned picker native resize readback")
        capture(output / f"{name}-resized.png")
    finally:
        resize(original["window"], original["size"])
        wait_for(lambda: matches(original["size"]), "owned picker original size restoration")
    return {"normal": original["size"], "resized": target, "restored": True}


def capture_screen(path):
    # A failed capture must not reuse an older image as fresh evidence.
    path.unlink(missing_ok=True)
    time.sleep(1)  # Allow the compositor to draw mode/size changes.
    subprocess.run(["screencapture", "-x", str(path)], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=20)
    try:
        with path.open("rb") as image:
            valid = image.read(8) == b"\x89PNG\r\n\x1a\n"
    except OSError:
        raise RuntimeError("Native appearance capture was not written") from None
    if not valid:
        raise RuntimeError("Native appearance capture is not a PNG")


def wait_for(read, description, seconds=20):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = read()
        if result:
            return result
        time.sleep(0.2)
    raise AssertionError(f"Timed out: {description}")


def preferences(value):
    if not isinstance(value, dict) or any(type(value.get(key)) is not bool for key in ("contrast", "transparency")):
        raise RuntimeError("Native accessibility preference state unavailable")
    return {key: value[key] for key in ("contrast", "transparency")}


def checked_preferences(value, required):
    actual = preferences(value)
    assert all(actual[key] == wanted for key, wanted in required.items()), "Native accessibility preferences changed unexpectedly"
    return actual


@contextmanager
def saved_preferences(read, write, restored=lambda: None, observe=lambda value: None):
    original = preferences(read())
    try:
        yield original
    finally:
        failed, attempts = False, []
        # Contrast can itself enable Reduce Transparency. Disable it first,
        # restore transparency, then restore contrast, verifying both at end.
        for key, value in (("contrast", False), ("transparency", original["transparency"]), ("contrast", original["contrast"])):
            try:
                write(key, value)
                stage = "verified"
            except Exception as error:
                failed = True
                stage = "helper-timeout" if isinstance(error, HelperTimeout) else (
                    "verification-deadline" if isinstance(error, AssertionError) else "action-failed")
            attempts.append({"key": key, "value": value, "stage": stage})
        final, matches = None, False
        try:
            final = preferences(read())
            matches = final == original
            failed |= not matches
        except Exception:
            failed = True
        observe({"attempts": attempts, "final_preferences": final, "matches_original": matches})
        if failed:
            raise RuntimeError("Original accessibility preferences could not be restored") from None
        restored()


def mode_facts(log):
    return re.findall(r"^ibis: native appearance mode=(opaque|vibrancy|mica) reason=(unsupported|not-transparent|unknown-preferences|accessibility|native-failure|applied)$", log, re.MULTILINE)


class NativeUI:
    def __init__(self):
        self.command = ["osascript", "-l", "JavaScript", str(Path(__file__).with_name("appearance-macos.js"))]
        self.owner = ""

    def call(self, action, value=""):
        actions = {"probe", "owner", "controls-ready", "read", "set", "closed", "close", "force-close",
                   "picker-owner", "picker-frame", "picker-size"}
        stages = {"identity", "registration", "process", "windows", "geometry", "ownership", "resize"}
        reasons = {"invalid-identity", "identity-changed", "ambiguous-window", "invalid-size", "native-call-failed"}
        if action not in actions:
            raise RuntimeError("Unknown native appearance action")
        def failed(stage, reason, kind=RuntimeError):
            facts = {"action": action, "stage": stage, "reason": reason}
            if not hasattr(self, "failure_facts"):
                self.failure_facts = facts
            print(f"NATIVE APPEARANCE FAILURE action={action} stage={stage} reason={reason}", flush=True)
            raise kind(f"Native appearance helper failed: {action}/{stage}/{reason}") from None
        if getattr(self, "last_action", None) != action:
            print("NATIVE APPEARANCE ACTION " + action, flush=True)
        self.last_action = action
        try:
            result = subprocess.run(self.command + [action, self.owner, value], encoding="utf-8",
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=25)
        except subprocess.TimeoutExpired:
            failed("invocation", "helper-timeout", HelperTimeout)
        if result.returncode:
            if any(marker in result.stderr for marker in ("TCC Accessibility is not granted", "not allowed assistive access", "(-1743)")):
                failed("invocation", "permission-unavailable", Unavailable)
            failed("invocation", "helper-failed")
        try:
            value = json.loads(result.stdout)
        except (ValueError, TypeError):
            failed("invocation", "invalid-response")
        if isinstance(value, dict) and "nativeAppearanceFailure" in value:
            facts = value["nativeAppearanceFailure"]
            if (set(value) != {"nativeAppearanceFailure"} or not isinstance(facts, dict)
                    or set(facts) != {"action", "stage", "reason"} or facts["action"] != action
                    or action not in {"picker-owner", "picker-frame", "picker-size"}
                    or type(facts["stage"]) is not str or facts["stage"] not in stages
                    or type(facts["reason"]) is not str or facts["reason"] not in reasons):
                failed("invocation", "invalid-response")
            failed(facts["stage"], facts["reason"])
        return value

    def read(self):
        return preferences(self.call("read"))

    def write(self, key, value):
        result = self.call("set", json.dumps({"key": key, "value": value}))
        if not isinstance(result, dict) or type(result.get("changed")) is not bool:
            raise RuntimeError("Native preference action metadata unavailable")
        if result["changed"]:
            wait_for(lambda: self.read()[key] == value, "ordinary accessibility preference change")

    def close(self, restoration_verified):
        self.cleanup_facts = {"requested": False, "terminated": False, "forced": False}
        def terminated():
            value = self.call("closed")
            if type(value) is not bool:
                raise RuntimeError("Owned System Settings termination metadata unavailable")
            self.cleanup_facts["terminated"] = value
            return value
        if terminated():
            return
        try:
            request = self.call("close")
        except Exception:
            if terminated():
                return
            raise
        if not isinstance(request, dict) or type(request.get("requested")) is not bool:
            raise RuntimeError("Owned System Settings termination metadata unavailable")
        self.cleanup_facts["requested"] = request["requested"]
        try:
            wait_for(terminated, "owned System Settings normal quit", seconds=5)
        except AssertionError:
            if terminated():
                return
            if not restoration_verified:
                raise RuntimeError("Owned System Settings force quit requires verified preference restoration") from None
            if self.call("force-close", json.dumps({"restoration_verified": True})) is not True:
                raise RuntimeError("Owned System Settings force quit metadata unavailable")
            self.cleanup_facts["forced"] = True
            wait_for(terminated, "owned System Settings force quit", seconds=5)


def stop_owned(process):
    if process and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    report = {"platform": platform.system(), "status": "failed", "checks": [], "restored": False}
    ui, app, backdrop = NativeUI(), None, None
    claimed = False
    try:
        if platform.system() != "Darwin":
            raise Unavailable("Native appearance scenario requires macOS")
        ui.call("probe")  # Refuses preexisting System Settings; no consent prompts.
        subprocess.run(["open", "x-apple.systempreferences:com.apple.Accessibility-Settings.extension?Display"],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=20)
        ui.owner = wait_for(lambda: ui.call("owner"), "new System Settings process")
        claimed = True
        wait_for(lambda: ui.call("controls-ready").get("ready") is True, "exact accessibility display controls")
        settings = Path.home() / "Library/Application Support/io.github.ibiscite.ibis/settings.json"
        if any(settings.with_suffix(suffix).exists() for suffix in (".json", ".json.bak", ".json.tmp")):
            raise Unavailable("Use a disposable desktop without existing Ibis settings")
        def restoration(value):
            report.update(restored=value["matches_original"], restoration=value)
        with saved_preferences(ui.read, ui.write, observe=restoration) as original:
            report["original_preferences"] = original
            ui.write("contrast", False)
            ui.write("transparency", False)
            with tempfile.TemporaryDirectory(prefix="ibis-appearance-") as folder:
                root = Path(folder)
                library = root / "synthetic.bib"
                library.write_text("@article{appearance1,title={Synthetic appearance reference},year={2026}}\n", encoding="utf-8")
                env = dict(os.environ, IBIS_LIBRARY=str(library))
                env.pop("IBIS_GALLERY", None)
                env.pop("IBIS_GALLERY_STATE", None)
                backdrop = subprocess.Popen(ui.command + ["backdrop", "", ""], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
                ready, _, _ = select.select([backdrop.stdout], [], [], 20)
                if not ready or backdrop.stdout.readline() != b'{"ready":true}\n':
                    raise RuntimeError("Owned appearance backdrop unavailable")
                log_path = args.output / "native-appearance.log"
                with log_path.open("w", encoding="utf-8") as log:
                    app = subprocess.Popen([str(args.app.resolve())], env=env, stdout=log, stderr=log)

                    def picker_owner():
                        return owned_picker_call(app, ui, "picker-owner", {"pid": app.pid, "executable": str(args.app.resolve())})
                    owner = wait_for(picker_owner, "owned picker process registration")

                    def raw_frame():
                        return owned_picker_call(app, ui, "picker-frame", {"owner": owner})
                    # Pin once for the whole five-phase transaction, so a
                    # replacement window cannot reset identity between pairs.
                    frame = pin_picker_window(raw_frame)

                    def resize(window, size):
                        result = owned_picker_call(app, ui, "picker-size", {"owner": owner, "window": window, "size": size})
                        if result is not True:
                            raise RuntimeError("Owned picker resize was not acknowledged")

                    def observe(name, expected, offset, required):
                        def selected():
                            if app.poll() is not None or backdrop.poll() is not None:
                                raise RuntimeError("Owned native appearance process exited")
                            data = log_path.read_text(encoding="utf-8")
                            if len(data) > 1024 * 1024:
                                raise RuntimeError("Native appearance log exceeded bound")
                            return expected in mode_facts(data[offset:])
                        wait_for(selected, "fixed native appearance mode change")
                        actual = checked_preferences(ui.read(), required)
                        # Keep the existing normal-size evidence and prove a
                        # real resize before capturing the same window again.
                        geometry = capture_sizes(name, args.output, frame, resize, capture_screen)
                        assert preferences(ui.read()) == actual, "Native accessibility preferences changed during capture"
                        report["checks"].append({"name": name, "mode": expected[0], "reason": expected[1],
                                                 "preferences": actual, "geometry": geometry})
                        print(f"PASS native appearance {name}", flush=True)

                    both_off = {"contrast": False, "transparency": False}
                    observe("material", ("vibrancy", "applied"), 0, both_off)
                    for name, key in (("reduce-transparency", "transparency"), ("increase-contrast", "contrast")):
                        offset = len(log_path.read_text(encoding="utf-8"))
                        ui.write(key, True)
                        # Increase Contrast may automatically enable Reduce
                        # Transparency. Record both actual states, rather than
                        # asserting those native settings are independent.
                        required = {key: True}
                        if key == "transparency":
                            required["contrast"] = False
                        observe(name, ("opaque", "accessibility"), offset, required)
                        offset = len(log_path.read_text(encoding="utf-8"))
                        ui.write("contrast", False)
                        ui.write("transparency", False)
                        observe(f"material-after-{name}", ("vibrancy", "applied"), offset, both_off)
        report["status"] = "observed"
        report["visual_review"] = "required: inspect all ten normal/resized images for material, outer clipping, content and native shadow; accessibility may change shadow"
    except Unavailable as error:
        report.update(status="unavailable", reason=str(error))
    except Exception as error:
        # Allow only our fixed failure classes; never print subprocess commands/errors.
        report["reason"] = str(error) if type(error) in (RuntimeError, AssertionError, HelperTimeout) else type(error).__name__
    finally:
        if hasattr(ui, "failure_facts"):
            report["helper_failure"] = dict(ui.failure_facts)
        for process in (app, backdrop):
            try:
                stop_owned(process)
            except Exception:
                report.update(status="failed", cleanup="Owned appearance process cleanup failed")
        if claimed:
            try:
                ui.close(report["restored"])  # Every action revalidates process identity.
            except Exception:
                report.update(status="failed", cleanup="Owned System Settings cleanup failed")
            finally:
                report["settings_cleanup"] = dict(ui.cleanup_facts)
        (args.output / "appearance.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print("Native appearance status: " + report["status"], flush=True)
    return {"observed": 0, "unavailable": 2}.get(report["status"], 1)


if __name__ == "__main__":
    raise SystemExit(main())
