#!/usr/bin/env python3
"""Bounded real CLI convergence/restart qualification: macOS + Docker Linux."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import queue
import re
import socket
import subprocess
import tarfile
import threading
import time
import uuid

ROOT = Path(__file__).resolve().parent.parent
COLLECTION = "0123456789abcdef0123456789abcdef"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, *, log=None, **kwargs):
    return subprocess.run(command, check=True, timeout=180, stdout=log, stderr=log, **kwargs)


def port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


class Peer:
    def __init__(self, command, log):
        self.lines = queue.Queue()
        self.log = log.open("a")
        self.child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=self.log, text=True, bufsize=1)

        def read():
            for line in self.child.stdout:
                self.lines.put(line.rstrip("\n"))
            self.lines.put(None)

        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        try:
            ready = self.line()
        except BaseException:
            self.close(kill=True)
            raise
        if not ready.startswith("ready "):
            self.close(kill=True)
            raise RuntimeError("peer failed to become ready")

    def line(self):
        try:
            line = self.lines.get(timeout=15)
        except queue.Empty as error:
            raise TimeoutError("peer command timed out") from error
        if line is None:
            raise RuntimeError("peer exited before replying")
        return line

    def command(self, command):
        self.child.stdin.write(command + "\n")
        self.child.stdin.flush()
        return self.line()

    def state(self):
        line = self.command("show")
        match = re.fullmatch(r"state (\d+) (\d+) (\[[\d, ]*\]) (\[[\d, ]*\]) (\[[\d, ]*\])", line)
        if not match:
            raise RuntimeError("invalid state reply: " + line)
        return [int(match[1]), int(match[2]), *[json.loads(match[i]) for i in (3, 4, 5)]]

    def acknowledgments(self):
        line = self.command("peer")
        match = re.fullmatch(r"peer (\d+) (?:ack (\d+)|idle|error .+)", line)
        if not match:
            raise RuntimeError("invalid peer reply: " + line)
        return int(match[1]), line, int(match[2]) if match[2] is not None else None

    def close(self, *, kill=False):
        if self.child.poll() is None:
            if kill:
                self.child.kill()
            else:
                self.child.stdin.write("quit\n")
                self.child.stdin.flush()
            try:
                code = self.child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.child.kill()
                self.child.wait()
                raise
            if not kill and code != 0:
                raise RuntimeError(f"peer exited {code}")
        self.child.stdin.close()
        self.reader.join(timeout=2)
        self.child.stdout.close()
        self.log.close()


def converge(a, b, expected):
    deadline = time.monotonic() + 25
    previous = None
    baseline = [0, 0]
    while time.monotonic() < deadline:
        states = [a.state(), b.state()]
        replies = [a.acknowledgments(), b.acknowledgments()]
        counts = [reply[0] for reply in replies]
        if all(state[1:] == expected for state in states) and all(reply[2] == expected[0] for reply in replies):
            revisions = [state[0] for state in states]
            if revisions != previous:
                previous, baseline = revisions, counts
            elif all(count >= start + 2 for count, start in zip(counts, baseline)):
                return states, counts
        else:
            previous = None
        time.sleep(0.05)
    raise TimeoutError(f"convergence/repeat-sync failed: {states}, {counts}, expected {expected}")


def paused(a, b):
    # push off stops future attempts, not an attempt already in flight. Observe
    # the three-second transport budget plus scheduling allowance before the
    # next offline edit; the edit also checks that no reply arrived meanwhile.
    deadline = time.monotonic() + 15
    counts = [a.acknowledgments()[0], b.acknowledgments()[0]]
    quiet_since = time.monotonic()
    while time.monotonic() < deadline:
        latest = [a.acknowledgments()[0], b.acknowledgments()[0]]
        if latest != counts:
            counts, quiet_since = latest, time.monotonic()
        elif time.monotonic() - quiet_since > 3.5:
            return
        time.sleep(0.1)
    raise TimeoutError("outgoing replies did not quiesce")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="New private evidence directory; retains test credentials")
    parser.add_argument("--rounds", type=int, default=30)
    parser.add_argument("--restart-every", type=int, default=5)
    parser.add_argument("--source", default="HEAD")
    parser.add_argument("--image", default="sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("this profile requires a native macOS client and Docker Linux")
    if not 1 <= args.rounds <= 365 or not 1 <= args.restart_every <= args.rounds:
        parser.error("rounds must be 1..365; restart-every must be 1..rounds")
    requested = Path(os.path.abspath(args.output))
    # Reject before creating Docker resources, generating credentials or building.
    if os.path.lexists(requested) or not requested.parent.is_dir():
        parser.error("output must be new with an existing parent")
    output = requested.parent.resolve() / requested.name
    source_id = subprocess.check_output(["git", "rev-parse", args.source + "^{commit}"], cwd=ROOT, text=True, timeout=30).strip()
    image = subprocess.check_output(["docker", "image", "inspect", args.image, "--format={{.Id}}"], text=True, timeout=30).strip()
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", image):
        raise RuntimeError("Docker image identity missing")
    output.mkdir(mode=0o700)
    source = output / "source"
    source.mkdir()
    archive = output / "source.tar"
    run(["git", "archive", "--output", str(archive), source_id], cwd=ROOT)
    with tarfile.open(archive) as bundle:
        bundle.extractall(source, filter="data")
    evidence = {"source_commit": source_id, "source_tar_sha256": digest(archive),
                "runner_sha256": digest(Path(__file__)), "docker_image": image,
                "platform": platform.platform(), "rounds_requested": args.rounds,
                "verified": False, "rounds": [], "credentials": "synthetic test keys retained privately; do not distribute output"}
    prefix = "weekend-study-" + uuid.uuid4().hex
    volume = prefix + "-data"
    native_port, linux_port = port(), port()
    while native_port == linux_port:
        linux_port = port()
    a = b = None

    def event(record):
        with (output / "events.jsonl").open("a") as stream:
            stream.write(json.dumps(record) + "\n")

    def container():
        run(["docker", "run", "-d", "--rm", "--name", prefix, "--memory=2g", "--pids-limit=256",
             "-p", f"127.0.0.1:{linux_port}:{linux_port}",
             "--mount", f"type=volume,source={volume},target=/data",
             "--mount", f"type=bind,source={source},target=/src,readonly",
             "--mount", f"type=bind,source={Path.home() / '.cargo/registry'},target=/usr/local/cargo/registry,readonly",
             image, "tail", "-f", "/dev/null"], log=build)

    def start_a():
        return Peer([str(binary), "run", str(output / "a.db"), "a", COLLECTION,
                     f"0.0.0.0:{native_port}", f"127.0.0.1:{linux_port}",
                     str(output / "a-identity"), str(output / "b-identity/cert.der"), "--no-push"], output / "a-stderr.log")

    def start_b():
        # SocketAddr requires a numeric address; resolve again after recreation.
        callback = subprocess.check_output(["docker", "exec", prefix, "getent", "ahostsv4", "host.docker.internal"], text=True, timeout=30).split()[0]
        return Peer(["docker", "exec", "-i", prefix, "/data/build/release/study-peer", "run", "/data/b.db", "b", COLLECTION,
                     f"0.0.0.0:{linux_port}", f"{callback}:{native_port}", "/data/b-identity", "/data/a-cert.der", "--no-push"], output / "b-stderr.log")

    with (output / "build.log").open("w") as build:
        try:
            env = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_TARGET_DIR=str(output / "native-build"), WEEKEND_DB_SOURCE_ID=source_id)
            run(["cargo", "build", "--offline", "--locked", "--release", "-p", "study-peer"], cwd=source, env=env, log=build)
            binary = output / "native-build/release/study-peer"
            evidence["native_binary_sha256"] = digest(binary)
            evidence["rustc"] = subprocess.check_output(["rustc", "-Vv"], text=True, timeout=30)
            evidence["native_filesystem"] = subprocess.check_output(["df", "-h", str(output)], text=True, timeout=30)
            device = evidence["native_filesystem"].splitlines()[-1].split()[0]
            evidence["native_volume"] = subprocess.check_output(["diskutil", "info", device], text=True, timeout=30)
            for actor in ["a", "b"]:
                run([str(binary), "keygen", str(output / f"{actor}-identity")], log=build)
            run([str(binary), "init", str(output / "a.db"), "a", COLLECTION], log=build)
            container()
            run(["docker", "exec", "-e", "CARGO_INCREMENTAL=0", "-e", f"WEEKEND_DB_SOURCE_ID={source_id}", prefix,
                 "cargo", "build", "--offline", "--locked", "--release", "--manifest-path", "/src/Cargo.toml", "-p", "study-peer", "--target-dir", "/data/build"], log=build)
            run(["docker", "cp", str(output / "b-identity"), prefix + ":/data/b-identity"], log=build)
            run(["docker", "cp", str(output / "a-identity/cert.der"), prefix + ":/data/a-cert.der"], log=build)
            run(["docker", "exec", prefix, "/data/build/release/study-peer", "init", "/data/b.db", "b", COLLECTION], log=build)
            evidence["linux_binary_sha256"] = subprocess.check_output(["docker", "exec", prefix, "sha256sum", "/data/build/release/study-peer"], text=True, timeout=30).split()[0]
            evidence["linux_platform"] = subprocess.check_output(["docker", "exec", prefix, "sh", "-c", "uname -a; rustc -Vv; findmnt -T /data -o SOURCE,FSTYPE,OPTIONS"], text=True, timeout=30)
            a = start_a()
            b = start_b()
            paused(a, b)
            total, lessons = 0, set()
            for round_id in range(1, args.rounds + 1):
                before = [a.state(), b.state()]
                counts = [a.acknowledgments()[0], b.acknowledgments()[0]]
                for peer, actor in [(a, 0), (b, 1)]:
                    lesson = (round_id * 2 + actor) % 64
                    lessons.add(lesson)
                    reply = peer.command(f"edit {lesson} {round_id * (actor + 1)} {100 * (actor + 1) + round_id} {20 - 10 * actor + round_id % 5}")
                    if reply != f"local {before[actor][0] + 1}":
                        raise RuntimeError("local commit revision mismatch: " + reply)
                    if peer.acknowledgments()[0] != counts[actor]:
                        raise RuntimeError("paused peer unexpectedly acknowledged an offline edit")
                total += 3 * round_id
                event({"phase": "local", "round": round_id, "states": [a.state(), b.state()], "prior_remote_ack_counts": counts})
                if a.command("push on") != "push on" or b.command("push on") != "push on":
                    raise RuntimeError("push admission failed")
                started = time.monotonic()
                states, acknowledgments = converge(a, b, [total, [200 + round_id], [10 + round_id % 5], sorted(lessons)])
                if a.command("push off") != "push off" or b.command("push off") != "push off":
                    raise RuntimeError("push suspension failed")
                paused(a, b)
                if [a.state(), b.state()] != states:
                    raise RuntimeError("state changed after converged outgoing attempts quiesced")
                record = {"round": round_id, "states": states, "remote_ack_counts": acknowledgments,
                          "converge_repeat_sync_seconds": time.monotonic() - started}
                if round_id % args.restart_every == 0:
                    # Native SIGKILL and container SIGKILL occur only after local
                    # commits and successful covering replies, not mid-publication.
                    a.close(kill=True)
                    a = None
                    run(["docker", "kill", prefix], log=build)
                    b.close(kill=True)
                    b = None
                    subprocess.run(["docker", "rm", "-f", prefix], stdout=build, stderr=build, timeout=30)
                    container()
                    a = start_a()
                    b = start_b()
                    reopened = [a.state(), b.state()]
                    if reopened != states:
                        raise RuntimeError(f"acknowledged state/revision lost on restart: {states} -> {reopened}")
                    record["restart_verified"] = True
                evidence["rounds"].append(record)
                event(dict(phase="converged", **record))
            a.close()
            a = None
            b.close()
            b = None
            run(["docker", "cp", prefix + ":/data/b.db", str(output / "b.db")], log=build)
            evidence["final_database_sha256"] = {actor: digest(output / f"{actor}.db") for actor in ["a", "b"]}
            evidence["verified"] = True
        finally:
            cleanup_errors = []
            for peer in [a, b]:
                if peer is not None:
                    try:
                        peer.close(kill=True)
                    except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
                        cleanup_errors.append(str(error))
            # Even a failed or timed-out daemon request must leave final failure
            # evidence. Absence is checked independently, including --rm races.
            evidence["cleanup"] = []
            for name, listing, removal in [
                (prefix, ["docker", "ps", "-a", "--format={{.Names}}"], ["docker", "rm", "-f", prefix]),
                (volume, ["docker", "volume", "ls", "--format={{.Name}}"], ["docker", "volume", "rm", volume]),
            ]:
                result = {"name": name, "absent": False}
                try:
                    names = subprocess.check_output(listing, text=True, timeout=30).splitlines()
                    if name in names:
                        removed = subprocess.run(removal, stdout=build, stderr=build, timeout=30)
                        result["removal_returncode"] = removed.returncode
                    names = subprocess.check_output(listing, text=True, timeout=30).splitlines()
                    result["absent"] = name not in names
                    if not result["absent"]:
                        cleanup_errors.append("resource remains: " + name)
                except (OSError, subprocess.SubprocessError) as error:
                    cleanup_errors.append(str(error))
                    result["error"] = str(error)
                evidence["cleanup"].append(result)
            if cleanup_errors:
                evidence["verified"] = False
                evidence["cleanup_errors"] = cleanup_errors
            (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
            if cleanup_errors:
                raise RuntimeError("peer cleanup failed: " + "; ".join(cleanup_errors))


if __name__ == "__main__":
    main()
