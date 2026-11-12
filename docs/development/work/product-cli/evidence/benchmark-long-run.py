# Copyright (c) 2026 kiryl contributors
"""Finite 120-second product observation; no universal bound is inferred."""
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
    files += [OUT / "benchmark-fixture.py", OUT / "benchmark-application.py", Path(__file__).resolve()]
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


def inventory(path):
    return {str(item.relative_to(path)): item.stat().st_size for item in path.rglob("*") if item.is_file()}


def metrics(path):
    sessions = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
    return {"completed_worker_summaries": len(sessions), "recorded_property_calls": sum(item["calls"] for item in sessions), "recorded_paths": sorted({name for item in sessions for name in item["paths"]})}


before = sources()
revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
environment = json.loads(subprocess.check_output([str(PYTHON), "-c", "import json,sys,hypothesis,pytest;print(json.dumps({'python':sys.version,'hypothesis':hypothesis.__version__,'pytest':pytest.__version__}))"], text=True))
with tempfile.TemporaryDirectory(prefix="kiryl-benchmark-long-") as temporary:
    work = Path(temporary)
    (work / "test_branches.py").write_bytes((OUT / "benchmark-fixture.py").read_bytes())
    (work / "application.py").write_bytes((OUT / "benchmark-application.py").read_bytes())
    summary = work / "sessions.jsonl"
    env = os.environ.copy()
    env["KIRYL_BENCH_METRICS"] = str(summary)
    report_path = OUT / "benchmark-long-product-report.json"
    command = [str(BINARY), "run", "test_branches.py", "--python", str(PYTHON), "--best-effort", "--duration", "120", "--seed", "17", "--store", str(work / "store"), "--report", str(report_path)]
    start = time.monotonic()
    samples = []
    process = None
    try:
        with (OUT / "benchmark-long-stdout.txt").open("w") as stdout, (OUT / "benchmark-long-stderr.txt").open("w") as stderr:
            process = subprocess.Popen(command, cwd=work, env=env, stdout=stdout, stderr=stderr, start_new_session=True)
            while process.poll() is None:
                elapsed = time.monotonic() - start
                files = inventory(work / "store")
                corpus = work / "store/target-0/corpus.json"
                try:
                    inputs = json.loads(corpus.read_text()) if corpus.exists() else []
                    corpus_count = len(inputs)
                    corpus_input_bytes = sum(len(item) for item in inputs)
                except (json.JSONDecodeError, FileNotFoundError):
                    corpus_count = None
                    corpus_input_bytes = None
                samples.append({"elapsed_s": round(elapsed,3), "rss_kib": sample_tree(process.pid), "artifact_bytes":sum(files.values()), "artifact_files":len(files), "corpus_entries":corpus_count, "corpus_input_bytes":corpus_input_bytes, **metrics(summary)})
                if elapsed > 140:
                    raise TimeoutError("120-second campaign exceeded 20-second finalization allowance")
                time.sleep(1)
    finally:
        if process is not None and process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
    elapsed = time.monotonic() - start
    files = inventory(work / "store")
    archive = OUT / "benchmark-long-store"
    archive.mkdir(exist_ok=True)
    for name in files:
        destination = archive / name
        destination.parent.mkdir(parents=True,exist_ok=True)
        destination.write_bytes((work / "store" / name).read_bytes())
    result = {"revision":revision,"working_tree":True,"platform":platform.platform(),"environment":environment,"requested_seconds":120,"seed":17,"sampling_interval_s":1,"command":command,"elapsed_s":elapsed,"exit_code":process.returncode,"source_sha256_before":before,"source_sha256_after":sources(),"samples":samples,"peak_sampled_tree_rss_kib":max(sample["rss_kib"] for sample in samples),"artifact_inventory":files,"artifact_bytes":sum(files.values()),"final_metrics":metrics(summary),"sessions":[json.loads(line) for line in summary.read_text().splitlines()] if summary.exists() else [],"report":json.loads(report_path.read_text()) if report_path.exists() else None}
    (OUT / "benchmark-long-results.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps({key:result[key] for key in ["elapsed_s","exit_code","peak_sampled_tree_rss_kib","artifact_bytes","final_metrics"]},indent=2))
