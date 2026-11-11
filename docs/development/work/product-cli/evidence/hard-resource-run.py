"""Required native product gates; absence of a hard backend is a failure."""

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

TARGET = r'''
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from hypothesis import given, strategies as st

LINGER = None

CHILD = r"""
import os
from pathlib import Path
import sys
import time
root = Path(sys.argv[1])
slot = sys.argv[2]
while not (root / 'go').exists():
    time.sleep(.01)
try:
    blocks = []
    for _ in range(192):
        block = bytearray(1024 * 1024)
        block[::4096] = b"x" * 256
        blocks.append(block)
    (root / (slot + '.ready')).touch()
    deadline = time.monotonic() + 15
    while not (root / 'release').exists() and time.monotonic() < deadline:
        time.sleep(.01)
except MemoryError:
    (root / (slot + '.denied')).touch()
    raise SystemExit(7)
"""

def transfer_handles(children):
    if sys.platform != 'win32':
        return []
    import ctypes
    from ctypes import wintypes
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    kernel.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.DuplicateHandle.argtypes = (wintypes.HANDLE, wintypes.HANDLE, wintypes.HANDLE, ctypes.POINTER(wintypes.HANDLE), wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    kernel.DuplicateHandle.restype = wintypes.BOOL
    kernel.CloseHandle.argtypes = (wintypes.HANDLE,)
    kernel.CloseHandle.restype = wintypes.BOOL
    observer = kernel.OpenProcess(0x0040, False, int(os.environ['KIRYL_HARD_OBSERVER_PID']))
    if not observer:
        raise ctypes.WinError(ctypes.get_last_error())
    handles = []
    try:
        for child in children:
            handle = wintypes.HANDLE()
            # CPython's Popen retains the exact process handle, including when
            # the child has exited. Duplicate into the external observer before
            # authorizing allocation, so PID reuse cannot invalidate cleanup.
            if not kernel.DuplicateHandle(kernel.GetCurrentProcess(), int(child._handle), observer, ctypes.byref(handle), 0x1000, False, 0):
                raise ctypes.WinError(ctypes.get_last_error())
            handles.append(handle.value)
    finally:
        kernel.CloseHandle(observer)
    return handles

@given(st.just(None))
def test_descendant_aggregate(_):
    global LINGER
    if LINGER is None:
        LINGER = subprocess.Popen([sys.executable, '-I', '-c', 'import time; time.sleep(120)'])
    root = Path(tempfile.mkdtemp(dir=os.environ['KIRYL_HARD_TRACE']))
    children = [subprocess.Popen([sys.executable, '-I', '-c', CHILD, str(root), str(i)]) for i in range(2)]
    (root / 'handles.json').write_text(json.dumps(transfer_handles([LINGER] + children)))
    (root / 'pids.json').write_text(json.dumps([LINGER.pid] + [child.pid for child in children]))
    (root / 'go').touch()
    try:
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if all((root / (str(i) + '.ready')).exists() or (root / (str(i) + '.denied')).exists() or children[i].poll() is not None for i in range(2)):
                break
            time.sleep(.01)
        ready = all((root / (str(i) + '.ready')).exists() for i in range(2))
        (root / 'release').touch()
        for child in children:
            child.wait(timeout=2)
        assert ready, 'descendant allocation refused or child terminated; cause requires backend evidence'
    finally:
        # On a hard kill this finally block cannot run; controller tree cleanup
        # must terminate both descendants independently of Python teardown.
        for child in children:
            if child.poll() is None:
                child.kill()
            child.wait()
'''


def alive_handle(handle):
    import ctypes
    from ctypes import wintypes

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetExitCodeProcess.argtypes = (
        wintypes.HANDLE,
        ctypes.POINTER(wintypes.DWORD),
    )
    kernel.GetExitCodeProcess.restype = wintypes.BOOL
    code = wintypes.DWORD()
    if not kernel.GetExitCodeProcess(handle, ctypes.byref(code)):
        raise ctypes.WinError(ctypes.get_last_error())
    return code.value == 259  # STILL_ACTIVE


def alive(pid):
    stat = Path(f"/proc/{pid}/stat")
    try:
        return stat.read_text().rsplit(")", 1)[1].split()[0] != "Z"
    except FileNotFoundError:
        return False


def check_run(binary, python, root, evidence, name, memory):
    trace = root / (name + "-trace")
    trace.mkdir()
    report = evidence / (name + ".json")
    environment = dict(
        os.environ,
        KIRYL_HARD_TRACE=str(trace),
        KIRYL_HARD_OBSERVER_PID=str(os.getpid()),
    )
    completed = subprocess.run(
        [
            str(binary),
            "run",
            "test_hard.py",
            "--python",
            str(python),
            "--duration",
            "8",
            "--case-timeout",
            "7",
            "--slice",
            "1",
            "--memory-mib",
            str(memory),
            "--store",
            str(evidence / (name + "-store")),
            "--report",
            str(report),
        ],
        cwd=root,
        env=environment,
        capture_output=True,
        timeout=40,
        check=False,
    )
    (evidence / (name + "-stderr.txt")).write_bytes(completed.stderr)
    assert completed.returncode in (0, 1), (completed.returncode, completed.stderr)
    value = json.loads(report.read_text())
    target = value["targets"][0]["report"]
    expected = "job-object" if sys.platform == "win32" else "cgroup-v2"
    assert target["capabilities"]["tree_memory_hard"] is True
    assert target["capabilities"]["backend"] == expected
    records = list(trace.glob("*/pids.json"))
    (evidence / (name + "-processes.json")).write_text(
        json.dumps(
            [
                {
                    "pids": json.loads(record.read_text()),
                    "ready": [path.name for path in record.parent.glob("*.ready")],
                    "denied": [path.name for path in record.parent.glob("*.denied")],
                }
                for record in records
            ],
            indent=2,
        )
    )
    assert records, "no descendant processes were exercised"
    pids = [pid for record in records for pid in json.loads(record.read_text())]
    assert len(pids) >= 3
    handles = (
        [
            handle
            for record in records
            for handle in json.loads((record.parent / "handles.json").read_text())
        ]
        if sys.platform == "win32"
        else []
    )
    if sys.platform == "win32":
        assert len(handles) == len(pids), (
            "every descendant must have an observer-owned handle"
        )
    survivors = lambda: (
        [handle for handle in handles if alive_handle(handle)]
        if sys.platform == "win32"
        else [pid for pid in pids if alive(pid)]
    )
    deadline = time.monotonic() + 5
    while survivors() and time.monotonic() < deadline:
        time.sleep(0.05)
    remaining = survivors()
    (evidence / (name + "-cleanup.json")).write_text(
        json.dumps(
            {
                "remaining": remaining,
                "identity": "owned-process-handle" if handles else "linux-proc-state",
            }
        )
    )
    if handles:
        import ctypes
        from ctypes import wintypes

        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CloseHandle.argtypes = (wintypes.HANDLE,)
        kernel.CloseHandle.restype = wintypes.BOOL
        for handle in handles:
            if not kernel.CloseHandle(handle):
                raise ctypes.WinError(ctypes.get_last_error())
    assert not remaining, "a descendant survived campaign cleanup"
    kinds = {outcome["kind"] for outcome in target["outcomes"]}
    if name == "control":
        assert value["targets"][0]["attempts"] > 0
        assert any(
            (record.parent / "0.ready").exists()
            and (record.parent / "1.ready").exists()
            for record in records
        ), "ample-memory control did not allocate both children"
        assert not kinds.intersection(
            {"failure", "oom", "native_crash", "infrastructure", "case_timeout"}
        ), kinds
    elif sys.platform == "linux":
        assert "oom" in kinds, kinds
        assert any(
            outcome["kind"] == "oom" and outcome["original"]
            for outcome in target["outcomes"]
        )
    else:
        denied = list(trace.glob("*/*.denied"))
        # Completion notification absence cannot prove absence of a limit event.
        # A recorded refused allocation or abnormal candidate remains explicit.
        assert (
            "oom" in kinds or ("failure" in kinds and denied) or "native_crash" in kinds
        ), kinds
        (evidence / "windows-classification.json").write_text(
            json.dumps(
                {
                    "kinds": sorted(kinds),
                    "allocation_denials": len(denied),
                    "complete_oom_classification": False,
                    "assumption": "native abnormal status alone does not establish OOM cause",
                },
                indent=2,
            )
        )
    return value


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    args.evidence.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="kiryl-hard-") as directory:
        root = Path(directory)
        (root / "test_hard.py").write_text(TARGET)
        check_run(
            args.binary.resolve(),
            args.python.absolute(),
            root,
            args.evidence.resolve(),
            "control",
            768,
        )
        check_run(
            args.binary.resolve(),
            args.python.absolute(),
            root,
            args.evidence.resolve(),
            "aggregate",
            256,
        )
    parent = os.environ.get("KIRYL_CGROUP_PARENT")
    if parent:
        assert not list(Path(parent).glob("kiryl-probe-*")), (
            "owned worker cgroups remain"
        )


if __name__ == "__main__":
    main()
