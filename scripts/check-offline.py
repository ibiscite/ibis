#!/usr/bin/env python3
"""Sample native Unix process sockets without treating missing tools as success.

Capture --record-baseline BEFORE launching Ibis. macOS WebKit XPC networking
helpers can be parented by launchd, so monitor newly appearing helpers too.
Existing helpers are excluded, and reports distinguish their uncertain ownership.
Only numeric socket endpoints and process IDs are printed, never process args.
"""
import argparse
import ipaddress
import json
from pathlib import Path
import platform
import re
import subprocess
import sys
import time


def command(args, allowed=(0,)):
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=20)
    if result.returncode not in allowed or result.stderr.strip():
        raise RuntimeError(f"{Path(args[0]).name} could not enumerate sockets/processes completely")
    return result.stdout


def parse_processes(text):
    processes = {}
    for line in text.splitlines():
        parts = line.strip().split(None, 2)
        if len(parts) != 3 or not parts[0].isdigit() or not parts[1].isdigit():
            raise ValueError("Unrecognized process enumeration row")
        pid, parent, name = parts
        processes[int(pid)] = {"parent": int(parent), "name": Path(name).name}
    if not processes:
        raise ValueError("Empty process enumeration")
    return processes


def processes():
    return parse_processes(command(["ps", "-axo", "pid=,ppid=,comm="]))


def selected(process_table, root, baseline, system):
    if root not in process_table:
        raise RuntimeError("Ibis exited during offline sampling")
    tree = {root}
    while True:
        next_tree = tree | {pid for pid, row in process_table.items() if row["parent"] in tree}
        if next_tree == tree:
            break
        tree = next_tree
    xpc = set()
    if system == "Darwin":
        xpc = {pid for pid, row in process_table.items()
               if pid not in baseline and row["name"] in ("com.apple.WebKit.Networking", "WebKit.Networking")}
    return tree | xpc, xpc - tree


def local(address):
    # Unspecified addresses are binding endpoints, not remote connections.
    # IPv6 link-local (fe80::/10) is deliberately external.
    ip = ipaddress.ip_address(address.split("%", 1)[0])
    if ip.version == 6 and ip.ipv4_mapped is not None:
        ip = ip.ipv4_mapped
    return ip.is_loopback or ip.is_unspecified


def endpoint(value):
    if value.startswith("["):
        address, separator, port = value[1:].rpartition("]:")
    else:
        address, separator, port = value.rpartition(":")
    if not separator or not port or (port != "*" and (not port.isdecimal() or int(port) > 65535)):
        raise ValueError("Unrecognized numeric socket endpoint")
    if address == "*":
        return None
    # Validate even endpoints that are not currently selected.
    return None if local(address) else value


def parse_ss(text, ids):
    found = set()
    for line in text.splitlines():
        fields = line.split()
        if len(fields) < 6 or fields[0] not in ("tcp", "udp"):
            raise ValueError("Unrecognized ss row")
        remote = endpoint(fields[5])
        owners = {int(pid) for pid in re.findall(r"pid=(\d+)", " ".join(fields[6:]))}
        # TIME-WAIT/orphan sockets and other users' sockets can have no owner.
        # They cannot be attributed to Ibis. sample() separately verifies it can
        # enumerate the selected processes' fd directories before trusting ss.
        for pid in owners & ids:
            if remote:
                found.add((pid, fields[0], remote))
    return found


def parse_lsof(text, ids):
    found = set()
    pid = None
    protocol = None
    descriptor = None
    has_name = False
    process_has_descriptor = False

    def finish_descriptor():
        if descriptor is not None and (protocol not in ("TCP", "UDP") or not has_name):
            raise ValueError("Incomplete lsof socket record")

    for line in text.splitlines():
        if not line:
            continue
        tag, value = line[0], line[1:]
        if tag == "p":
            finish_descriptor()
            if pid is not None and not process_has_descriptor:
                raise ValueError("Incomplete lsof process record")
            if not value.isdigit():
                raise ValueError("Unrecognized lsof PID")
            pid, protocol, descriptor = int(value), None, None
            has_name = False
            process_has_descriptor = False
        elif tag == "f":
            finish_descriptor()
            if pid is None or not re.fullmatch(r"\d+[rwu]?", value):
                raise ValueError("Unrecognized lsof descriptor")
            descriptor = value
            protocol = None
            has_name = False
            process_has_descriptor = True
        elif tag == "P":
            if descriptor is None or protocol is not None or value not in ("TCP", "UDP"):
                raise ValueError("Unrecognized lsof protocol")
            protocol = value
        elif tag == "n":
            if pid is None or descriptor is None or protocol not in ("TCP", "UDP") or has_name:
                raise ValueError("Unrecognized lsof socket attribution")
            has_name = True
            if "->" in value:
                binding, peer = value.split("->", 1)
                endpoint(binding)
                remote = endpoint(peer)
            else:
                endpoint(value)  # Validate listening/unconnected names too.
                remote = None
            if remote and pid in ids:
                found.add((pid, protocol.lower(), remote))
        elif tag in ("t", "T"):
            if descriptor is None:
                raise ValueError("lsof metadata has no socket descriptor")
        else:
            raise ValueError("Unrecognized lsof field")
    finish_descriptor()
    if pid is not None and not process_has_descriptor:
        raise ValueError("Incomplete lsof process record")
    return found


def sample(system, ids):
    if system == "Linux":
        for pid in ids:
            try:
                list(Path(f"/proc/{pid}/fd").iterdir())
            except FileNotFoundError:
                continue  # A short-lived child may exit between snapshots.
        return parse_ss(command(["ss", "-H", "-t", "-u", "-a", "-n", "-p"]), ids)
    # lsof exits 1 with empty stdout/stderr when no selected process owns an
    # Internet socket. All diagnostics (including permission warnings) fail.
    output = command(["lsof", "-nP", "-a", "-p", ",".join(map(str, sorted(ids))), "-i", "-FpfPntT"], allowed=(0, 1))
    return parse_lsof(output, ids)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--record-baseline", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--pid", type=int)
    parser.add_argument("--seconds", type=float, default=60)
    parser.add_argument("--interval", type=float, default=1)
    args = parser.parse_args()
    try:
        system = platform.system()
        if system not in ("Linux", "Darwin"):
            raise RuntimeError("Use check-offline.ps1 on Windows")
        if args.record_baseline:
            args.record_baseline.write_text(json.dumps({"platform": system, "pids": sorted(processes())}) + "\n", encoding="utf-8")
            print("Captured process baseline before Ibis startup")
            return 0
        if not args.pid or not args.baseline or args.seconds <= 0 or args.interval <= 0:
            parser.error("sampling requires --pid, --baseline and positive durations")
        baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
        if baseline["platform"] != system or not baseline["pids"] or args.pid in baseline["pids"]:
            raise ValueError("Baseline must be captured on this platform before launching Ibis")
        seen = set()
        sampled = set()
        deadline = time.monotonic() + args.seconds
        while True:
            ids, helpers = selected(processes(), args.pid, set(baseline["pids"]), system)
            sampled.update(ids)
            for pid, protocol, remote in sample(system, ids):
                kind = "new WebKit helper (ownership unproven)" if pid in helpers else "Ibis process tree"
                record = (pid, protocol, remote)
                if record not in seen:
                    print(f"External socket: {kind}, pid={pid}, {protocol}, remote={remote}", flush=True)
                seen.add(record)
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            time.sleep(min(args.interval, remaining))
        if seen:
            raise RuntimeError(f"Offline session observed {len(seen)} external socket(s)")
        print(f"No external sockets observed in {args.seconds:g}s across {len(sampled)} sampled process IDs")
        return 0
    except (RuntimeError, ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
        # Known fixed diagnostics only; do not print raw subprocess exceptions.
        reason = str(error) if isinstance(error, (RuntimeError, ValueError)) else type(error).__name__
        print(f"Offline check failed: {reason}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
