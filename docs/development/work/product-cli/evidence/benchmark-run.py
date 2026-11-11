# Copyright (c) 2026 kiryl contributors
"""Run isolated finite observations; results do not imply performance superiority."""
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[5]
OUT = Path(__file__).resolve().parent
PYTHON = ROOT / "python/.venv/bin/python"
BINARY = ROOT / "target/debug/kiryl"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources():
    files = [ROOT / "Cargo.lock", ROOT / "python/uv.lock", BINARY]
    files += sorted((ROOT / "src").rglob("*.rs"))
    files += sorted((ROOT / "python/kiryl_adapter").glob("*.py"))
    files += [OUT / "benchmark-fixture.py", OUT / "benchmark-application.py", OUT / "benchmark-run.py"]
    return {str(path.relative_to(ROOT)): digest(path) for path in files}


def sample_tree(pid):
    rows = subprocess.check_output(["ps", "-axo", "pid=,ppid=,rss="], text=True)
    processes = [tuple(map(int, row.split())) for row in rows.splitlines()]
    selected = {pid}
    while True:
        expanded = selected | {child for child, parent, _ in processes if parent in selected}
        if expanded == selected:
            break
        selected = expanded
    return sum(rss for child, _, rss in processes if child in selected)


def disk_bytes(path):
    return sum(item.stat().st_size for item in path.rglob("*") if item.is_file())


def phase(name, command, cwd, metrics, stop_after=None):
    env = os.environ.copy()
    env["KIRYL_BENCH_METRICS"] = str(metrics)
    start = time.monotonic()
    with (OUT / f"benchmark-{name}.stdout").open("w") as stdout, (OUT / f"benchmark-{name}.stderr").open("w") as stderr:
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=stdout, stderr=stderr, start_new_session=True)
        samples = []
        interrupted = False
        while process.poll() is None:
            elapsed = time.monotonic() - start
            samples.append({"elapsed_s": round(elapsed, 3), "rss_kib": sample_tree(process.pid), "stored_bytes": disk_bytes(cwd)})
            if stop_after is not None and elapsed >= stop_after and not interrupted:
                os.killpg(process.pid, signal.SIGINT)
                interrupted = True
            if elapsed > (stop_after or 30) + 15:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                raise RuntimeError(f"{name} exceeded finalization allowance")
            time.sleep(0.5)
    result = {"command": command, "elapsed_s": time.monotonic() - start, "exit_code": process.returncode, "interrupt_at_deadline": interrupted, "samples": samples, "peak_sampled_tree_rss_kib": max(sample["rss_kib"] for sample in samples), "stored_bytes": disk_bytes(cwd)}
    result["sessions"] = [json.loads(line) for line in metrics.read_text().splitlines()] if metrics.exists() else []
    result["observed_property_calls"] = sum(item["calls"] for item in result["sessions"])
    result["observed_paths"] = sorted({path for item in result["sessions"] for path in item["paths"]})
    return result


before = sources()
revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
environment = subprocess.check_output([str(PYTHON), "-c", "import json,sys,hypothesis,pytest; print(json.dumps({'python':sys.version,'hypothesis':hypothesis.__version__,'pytest':pytest.__version__}))"], text=True)
with tempfile.TemporaryDirectory(prefix="kiryl-benchmark-") as temporary:
    work = Path(temporary)
    product = work / "product"
    ordinary = work / "ordinary"
    for directory in [product, ordinary]:
        directory.mkdir()
        (directory / "test_branches.py").write_bytes((OUT / "benchmark-fixture.py").read_bytes())
        (directory / "application.py").write_bytes((OUT / "benchmark-application.py").read_bytes())
    report_path = OUT / "benchmark-product-report.json"
    product_result = phase("product", [str(BINARY), "run", "test_branches.py", "--python", str(PYTHON), "--best-effort", "--duration", "30", "--seed", "17", "--store", str(product / "store"), "--report", str(report_path)], product, product / "sessions.jsonl")
    product_result["artifact_bytes"] = disk_bytes(product / "store")
    product_result["artifact_inventory"] = {str(item.relative_to(product / "store")): item.stat().st_size for item in (product / "store").rglob("*") if item.is_file()}
    wrapper = ordinary / "run_ordinary.py"
    wrapper.write_text('from hypothesis import settings, seed\nfrom hypothesis.database import DirectoryBasedExampleDatabase\nimport test_branches\ntry:\n    seed(17)(settings(max_examples=10**9, deadline=None, database=DirectoryBasedExampleDatabase("database"))(test_branches.test_integer_branches))()\nexcept KeyboardInterrupt:\n    pass\n')
    ordinary_result = phase("ordinary", [str(PYTHON), str(wrapper)], ordinary, ordinary / "sessions.jsonl", stop_after=30)
    ordinary_result["database_bytes"] = disk_bytes(ordinary / "database")
    if report_path.exists():
        product_result["report"] = json.loads(report_path.read_text())
    result = {"revision": revision, "working_tree": True, "platform": platform.platform(), "environment": json.loads(environment), "seed":17,"requested_seconds_per_phase":30,"rss_sampling_interval_s":0.5,"source_sha256_before":before,"source_sha256_after":sources(),"product":product_result,"ordinary":ordinary_result}
    (OUT / "benchmark-results.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({name:{key:value for key,value in result[name].items() if key not in {"samples","report","sessions","command"}} for name in ["product","ordinary"]},indent=2))
