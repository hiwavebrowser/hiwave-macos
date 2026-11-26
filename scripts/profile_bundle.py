"""Measure a local JS file in fresh release profile_bundle children.

Build with cargo build --release -p rustkit-bindings --example profile_bundle --features profile-bundle.
Windows requires pywin32 for Job Objects. Linux uses RLIMIT_AS. macOS checks RSS
periodically. Peak working set/RSS includes DOM startup and the input source.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import tempfile
import time


def windows_child(argv, timeout, limit):
    import win32api
    import win32con
    import win32job
    import win32process

    job = win32job.CreateJobObject(None, "")
    try:
        opts = win32job.QueryInformationJobObject(job, win32job.JobObjectExtendedLimitInformation)
        opts["BasicLimitInformation"]["LimitFlags"] |= (
            win32job.JOB_OBJECT_LIMIT_PROCESS_MEMORY | win32job.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        )
        opts["ProcessMemoryLimit"] = limit
        win32job.SetInformationJobObject(job, win32job.JobObjectExtendedLimitInformation, opts)
        with subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT) as child:
            rights = win32con.PROCESS_SET_QUOTA | win32con.PROCESS_TERMINATE | win32con.PROCESS_QUERY_INFORMATION | win32con.PROCESS_VM_READ
            try:
                process = win32api.OpenProcess(rights, False, child.pid)
                try:
                    win32job.AssignProcessToJobObject(job, process)
                    output, _ = child.communicate(b"1", timeout=timeout)
                    peak = win32process.GetProcessMemoryInfo(process)["PeakWorkingSetSize"]
                finally:
                    process.Close()
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait()
            return child.returncode, output, peak
    finally:
        job.Close()


def posix_child(argv, timeout, limit):
    import resource
    def limit_address_space():
        resource.setrlimit(resource.RLIMIT_AS, (limit, limit))

    with tempfile.TemporaryFile() as log, subprocess.Popen(
        argv, stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT,
        preexec_fn=limit_address_space if sys.platform.startswith("linux") else None,
    ) as child:
        try:
            child.stdin.write(b"1")
            child.stdin.close()
            deadline = time.monotonic() + timeout
            while True:
                pid, status, usage = os.wait4(child.pid, os.WNOHANG)
                if pid:
                    child.returncode = os.waitstatus_to_exitcode(status)
                    break
                if time.monotonic() >= deadline:
                    raise subprocess.TimeoutExpired(argv, timeout)
                if sys.platform == "darwin":
                    rss = subprocess.run(["ps", "-o", "rss=", "-p", str(child.pid)], capture_output=True, timeout=1)
                    if rss.stdout.strip() and int(rss.stdout) * 1024 > limit:
                        raise RuntimeError("RSS limit exceeded")
                time.sleep(0.025)
        finally:
            if child.returncode is None:
                child.kill()
                child.wait()
        log.seek(0)
        return child.returncode, log.read(), usage.ru_maxrss * (1 if sys.platform == "darwin" else 1024)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("child", type=Path)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--html", type=Path, help="initial document markup; its scripts are not run")
    parser.add_argument("--url", default="https://example.test/")
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--timeout", type=int, default=60, help="wall seconds per process")
    parser.add_argument("--memory-mib", type=int, default=2048)
    args = parser.parse_args()
    if min(args.runs, args.timeout, args.memory_mib) < 1:
        parser.error("limits and run count must be positive")
    argv = [str(args.child.resolve(strict=True)), str(args.bundle.resolve(strict=True)), args.url]
    if args.html:
        argv.append(str(args.html.resolve(strict=True)))
    with args.bundle.open("rb") as file:
        digest = hashlib.file_digest(file, "sha256").hexdigest()
    size = args.bundle.stat().st_size
    print(json.dumps(dict(bytes=size, sha256=digest, runs=args.runs)), flush=True)
    run = windows_child if os.name == "nt" else posix_child
    samples = []
    for _ in range(args.runs):
        code, output, peak = run(argv, args.timeout, args.memory_mib * 1024**2)
        if code:
            raise RuntimeError(f"child exited {code}: {output[-512:].decode(errors='replace')}")
        lines = output.decode(errors="replace").splitlines()
        result = next(json.loads(line.removeprefix("BUNDLE_RESULT ")) for line in lines if line.startswith("BUNDLE_RESULT "))
        sample = {"peak_rss_mib": peak / 1024**2}
        for phase, length, micros in re.findall(r'phase="(\w+)" bytes=(\d+) elapsed_us=(\d+)', "\n".join(lines)):
            if int(length) == size:
                sample[phase + "_ms"] = int(micros) / 1000
        if "parse_ms" not in sample:
            raise RuntimeError("no phase measurements found")
        print(json.dumps({**sample, **result}), flush=True)
        samples.append(sample)
    for key in samples[0]:
        values = [s[key] for s in samples if key in s]
        print(json.dumps(dict(metric=key, samples=len(values), median=statistics.median(values), min=min(values), max=max(values))))


if __name__ == "__main__":
    main()
