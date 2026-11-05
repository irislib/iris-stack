#!/usr/bin/env python3
"""Reuse one reviewed Iris compiler/snapshot installer; hosted product jobs only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess

SOURCE = "061c1747575e216db4506d12f2d358dd8c8e0241"
PINS = {'scripts/release-build-inputs.py': '44eb1fa2a1cca4982ab64d725e5ead39d3869a4dd5b5ebe941033901626b8351', 'scripts/release-build-inputs.json': 'b3b045ada1c9dcc0a59b8c38b0a0833c201c2f5a379c820306a829b493948a18', 'linux/scripts/pinned-apt.sh': '56a02b5f5dd4c73d754a72edbf83fdd5925bad72746f295a9c1f0152aa5af22b', 'linux/build-inputs/apt-snapshots.tsv': 'fe935df9e9a090d6d634d3e7e076b41c10225b3435778be984907f8bc8b1fc77'}
PACKAGES = ("libdbus-1-dev", "pkg-config", "protobuf-compiler")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def output(command):
    return subprocess.check_output(command, text=True, timeout=30).strip()


def verify_source(inputs):
    require(output(["git", "-C", str(inputs), "rev-parse", "HEAD"]) == SOURCE,
            "Build input checkout is not the reviewed source")
    require(not output(["git", "-C", str(inputs), "status", "--porcelain", "--untracked-files=all"]),
            "Build input checkout is dirty")
    for name, expected in PINS.items():
        path = inputs / name
        require(path.is_file() and not path.is_symlink()
                and hashlib.sha256(path.read_bytes()).hexdigest() == expected,
                "Build input file differs from reviewed source: " + name)


def verify_compiler(work):
    compiler = json.loads((work / "rust/receipt.json").read_text())
    require(compiler.get("passed") is True and compiler.get("sourceCommit") == SOURCE
            and compiler.get("host") == "x86_64-unknown-linux-gnu",
            "Compiler installer did not validate the reviewed host/source")
    require(re.search(r"^release: 1\.99\.0$", compiler["versions"]["RUSTC"], re.M)
            and compiler["versions"]["CARGO"].startswith("cargo 1.99.0 "),
            "Compiler installer receipt has the wrong toolchain")
    # The installer verifies the bytes and writes exports only after success.
    # Bind the next workflow step to those tools instead of a runner rustup shim.
    exports = dict(line.split("=", 1) for line in Path(os.environ["GITHUB_ENV"]).read_text().splitlines()
                   if "=" in line)
    for key in ("CARGO", "RUSTC", "RUSTDOC"):
        expected = work / "rust/toolchain/bin" / key.lower()
        require(exports.get(key) == str(expected) and expected.is_file()
                and os.access(expected, os.X_OK), "Compiler export has the wrong tool path: " + key)
    path_entries = Path(os.environ["GITHUB_PATH"]).read_text().splitlines()
    require(path_entries and path_entries[-1] == str(work / "rust/toolchain/bin"),
            "Verified compiler must be the first PATH entry in following steps")
    return compiler


def setup(inputs, work):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "Setup is restricted to hosted CI")
    require(platform.system() == "Linux" and platform.machine() == "x86_64",
            "Product setup requires the Ubuntu x86_64 runner")
    os_release = Path("/etc/os-release").read_text()
    require(re.search(r'^ID=ubuntu$', os_release, re.M)
            and re.search(r'^VERSION_ID="24\.04"$', os_release, re.M),
            "Product setup requires Ubuntu 24.04")
    require(work.is_absolute() and not any(c in str(work) for c in "\r\n"), "Safe absolute work path required")
    work.mkdir(mode=0o700, parents=True, exist_ok=False)
    receipt = {"passed": False, "sourceCommit": SOURCE, "inputSHA256": PINS,
               "workflowCommit": os.environ.get("GITHUB_SHA"),
               "runnerImage": {key: os.environ.get(key) for key in ("ImageOS", "ImageVersion")},
               "osRelease": os_release, "packages": {}}
    try:
        verify_source(inputs)
        subprocess.run(["python3", str(inputs / "scripts/release-build-inputs.py"),
                        "--install", "--work", str(work / "rust")], check=True)
        compiler = verify_compiler(work)
        receipt["compilerReceiptSHA256"] = hashlib.sha256((work / "rust/receipt.json").read_bytes()).hexdigest()
        receipt["compilerVersions"] = compiler["versions"]
        # Retain the original package roots. The helper verifies snapshot
        # signatures/hashes and package versions; no installer logic is copied.
        subprocess.run(["sudo", "bash", str(inputs / "linux/scripts/pinned-apt.sh"),
                        "ubuntu-noble", *PACKAGES], check=True)
        for package in PACKAGES:
            receipt["packages"][package] = output(["dpkg-query", "-W", "-f=${Version}", package])
        receipt["nativeVersions"] = {"pkg-config": output(["/usr/bin/pkg-config", "--version"]),
                                     "protoc": output(["/usr/bin/protoc", "--version"])}
        with Path(os.environ["GITHUB_ENV"]).open("a") as stream:
            stream.write("PKG_CONFIG=/usr/bin/pkg-config\nPROTOC=/usr/bin/protoc\n")
        receipt["passed"] = True
    except Exception as error:
        receipt["error"] = type(error).__name__ + ": " + str(error)
        raise
    finally:
        (work / "setup.json").write_text(json.dumps(receipt, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    setup(args.inputs.resolve(), args.work)


if __name__ == "__main__":
    main()
