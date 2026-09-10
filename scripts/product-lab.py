#!/usr/bin/env python3
"""Install exact product sources and run the process tests owned by this lab."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
PUBLIC_SOURCES = {
    "hashtree": "https://github.com/mmalmi/hashtree",
    "drive": "htree://npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-drive",
    "chat": "https://github.com/irislib/iris-chat-rs",
}
PRODUCTS = [
    ("hashtree", "HTREE", "hashtree-cli", "fips-webrtc,git-remote-wrapper", ["htree", "git-remote-htree"], ""),
    ("drive", "DRIVE", "iris-drive-core", "stack-fixture", ["iris-drive-stack-fixture"], "05751a828f2a20b3ed46d09569e6cf35ac4c537d"),
    ("chat", "CHAT", "iris-chat", "stack-fixture", ["iris-chat-stack-fixture"], "a4cafb1bb382593c9886d0a4314cf80292ef7850"),
]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    env = os.environ.copy()
    install = Path(env.get("IRIS_STACK_PRODUCT_INSTALL_ROOT", ROOT / "target/product-lab")).resolve()
    install.mkdir(parents=True, exist_ok=True)
    receipt_path, metrics_path = install / "receipt.json", install / "mesh-metrics.json"
    # A previous success must never survive a failed invocation at this location.
    receipt_path.unlink(missing_ok=True)
    metrics_path.unlink(missing_ok=True)
    env["IRIS_STACK_MESH_METRICS_PATH"] = str(metrics_path)
    release = env.get("IRIS_STACK_RELEASE_GATE") == "1"
    if release:
        env["IRIS_STACK_REQUIRE_CPU"] = "1"
        env["IRIS_STACK_IDLE_MAX_CPU_PERCENT"] = "5"
    plans = []
    for name, key, package, features, binaries, default_rev in PRODUCTS:
        prefix = f"IRIS_STACK_{key}"
        binary_key = prefix + ("_BIN" if name == "hashtree" else "_FIXTURE_BIN")
        binary = env.get(binary_key, "")
        source = env.get(prefix + "_GIT") or PUBLIC_SOURCES[name]
        rev = env.get(prefix + "_REV") or default_rev
        version = env.get("IRIS_STACK_HTREE_VERSION", "") if name == "hashtree" else ""
        require(not (version and rev), "Select htree_rev or htree_version, not both")
        if not rev and name == "hashtree":
            version = version or "0.2.146"
        require(not rev or re.fullmatch(r"[0-9a-f]{40}", rev), f"{prefix}_REV must be an exact 40-character commit")
        require(not version or re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?", version), "htree_version must be one exact version")
        if release:
            require(not binary, "Release gates install public sources; local binary overrides are not allowed")
            require(source == PUBLIC_SOURCES[name], "Release gates require the canonical public product source")
        provenance = {"source": "local-binary"} if binary else (
            {"source": "crates.io", "version": version} if version else {"source": source, "rev": rev})
        plans.append((name, package, features, binaries, binary_key, binary, provenance))

    lab_revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT, text=True).strip())
    expected_lab = env.get("IRIS_STACK_LAB_REV", "")
    require(not release or re.fullmatch(r"[0-9a-f]{40}", expected_lab), "Release gates require an exact IRIS_STACK_LAB_REV")
    require(not expected_lab or lab_revision == expected_lab, "Lab checkout does not match the requested revision")
    require(not release or not dirty, "Release gate lab sources must be clean")
    env["PATH"] = str(install / "bin") + os.pathsep + env["PATH"]
    products = {}
    for name, package, features, binaries, binary_key, binary, provenance in plans:
        if not binary:
            if provenance["source"].startswith("htree://") and not (install / "bin/git-remote-htree").is_file():
                subprocess.run(["cargo", "install", "--locked", "--root", str(install), "--version", "=0.2.89", "--bin", "git-remote-htree", "git-remote-htree"], check=True, env=env)
            command = ["cargo", "install", "--locked", "--root", str(install)]
            command += (["--version", "=" + provenance["version"]] if "version" in provenance else ["--git", provenance["source"], "--rev", provenance["rev"]])
            command += ["--features", features]
            for item in binaries:
                command += ["--bin", item]
            command.append(package)
            subprocess.run(command, check=True, env={**env, "CARGO_NET_GIT_FETCH_WITH_CLI": "true"})
            binary = str(install / "bin" / binaries[0])
        require(os.access(binary, os.X_OK), f"Product binary is not executable: {binary}")
        env[binary_key] = str(Path(binary).resolve())
        digest = hashlib.sha256()
        with open(binary, "rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
        products[name] = {**provenance, "sha256": digest.hexdigest()}

    for gate in ["drive_htree_product", "chat_drive_htree_product", "relayless_mesh_product"]:
        subprocess.run(["cargo", "test", "--locked", "--test", gate, "--", "--ignored", "--nocapture"], cwd=ROOT, check=True, env=env)
    metrics = json.loads(metrics_path.read_text())
    require(isinstance(metrics, dict) and isinstance(metrics.get("idle"), list)
            and len(metrics["idle"]) == 2
            and all(isinstance(sample, dict) and isinstance(sample.get("cpu_percent"), list)
                    and len(sample["cpu_percent"]) == 3 for sample in metrics["idle"]),
            "Malformed mesh resource measurements")
    receipt = {"schema_version": 1, "status": "passed", "lab_revision": lab_revision, "lab_worktree_clean": not dirty,
               "release_gate": release, "products": products, "metrics": metrics}
    temporary = receipt_path.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(receipt, indent=2) + "\n")
    temporary.replace(receipt_path)
    print(f"Product gate receipt: {receipt_path}")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(error.returncode if error.returncode >= 0 else 128 - error.returncode)
    except (OSError, ValueError) as error:
        sys.exit(str(error))
