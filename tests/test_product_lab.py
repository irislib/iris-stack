import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SHA = "a" * 40


class ProductLabTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.calls = self.root / "calls.jsonl"
        self.install = self.root / "install"
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("IRIS_STACK_")}
        self.env.update(PATH=f"{self.bin}:{self.env['PATH']}",
                        IRIS_STACK_PRODUCT_INSTALL_ROOT=str(self.install),
                        TEST_CALLS=str(self.calls), TEST_RUNTIME_ENV=str(self.root / "runtime-env.json"))
        self.executable("cargo", '''#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
with open(os.environ["TEST_CALLS"], "a") as f:
    f.write(json.dumps(args) + "\\n")
if os.environ.get("TEST_FAIL") in args:
    sys.exit(23)
if args[0] == "install":
    root = pathlib.Path(args[args.index("--root") + 1]) / "bin"
    root.mkdir(parents=True, exist_ok=True)
    for i, arg in enumerate(args):
        if arg == "--bin":
            binary = root / args[i + 1]
            binary.write_text("#!/bin/sh\\nexit 0\\n")
            binary.chmod(0o755)
if args[0] == "test":
    pathlib.Path(os.environ["TEST_RUNTIME_ENV"]).write_text(json.dumps({
        key: os.environ.get(key) for key in ["IRIS_STACK_REQUIRE_CPU", "IRIS_STACK_IDLE_MAX_CPU_PERCENT"]}))
if "relayless_mesh_product" in args and not os.environ.get("TEST_NO_METRICS"):
    pathlib.Path(os.environ["IRIS_STACK_MESH_METRICS_PATH"]).write_text(
        os.environ.get("TEST_BAD_METRICS") or json.dumps({
            "public_relays": 0, "idle": [{"cpu_percent": [0.5, 0.5, 0.5]}] * 2}))
''')

    def executable(self, name, source):
        path = self.bin / name
        path.write_text(source)
        path.chmod(0o755)
        return path

    def run_lab(self, **env):
        return subprocess.run(["sh", str(ROOT / "scripts/product-lab.sh")],
                              env={**self.env, **env}, capture_output=True,
                              text=True, timeout=10)

    def commands(self):
        return [json.loads(line) for line in self.calls.read_text().splitlines()]

    def test_default_tuple_and_success_provenance(self):
        result = self.run_lab()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.commands()
        self.assertIn("=0.2.146", calls[0])
        self.assertEqual([c[c.index("--test") + 1] for c in calls if c[0] == "test"],
                         ["drive_htree_product", "chat_drive_htree_product", "relayless_mesh_product"])
        receipt = json.loads((self.install / "receipt.json").read_text())
        self.assertEqual(receipt["products"]["hashtree"]["version"], "0.2.146")
        self.assertEqual(receipt["metrics"]["public_relays"], 0)
        self.assertEqual(len(receipt["products"]["chat"]["sha256"]), 64)

    def test_hashtree_candidate_uses_exact_public_source_and_locked_install(self):
        result = self.run_lab(IRIS_STACK_HTREE_REV=SHA)
        self.assertEqual(result.returncode, 0, result.stderr)
        command = self.commands()[0]
        self.assertIn("--locked", command)
        self.assertIn("https://github.com/mmalmi/hashtree", command)
        self.assertEqual(command[command.index("--rev") + 1], SHA)
        self.assertNotIn("--version", command)
        receipt = json.loads((self.install / "receipt.json").read_text())
        self.assertEqual(receipt["products"]["hashtree"]["rev"], SHA)

    def test_invalid_or_conflicting_pins_fail_before_install(self):
        for overrides in [dict(IRIS_STACK_CHAT_REV="main"),
                          dict(IRIS_STACK_DRIVE_REV="a" * 39),
                          dict(IRIS_STACK_HTREE_REV="v0.2.146"),
                          dict(IRIS_STACK_HTREE_VERSION="*"),
                          dict(IRIS_STACK_HTREE_REV=SHA, IRIS_STACK_HTREE_VERSION="0.2.146")]:
            with self.subTest(overrides=overrides):
                result = self.run_lab(**overrides)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.calls.exists())

    def test_failed_gate_removes_stale_success_and_preserves_failure(self):
        self.install.mkdir()
        (self.install / "receipt.json").write_text('{"status":"passed"}')
        result = self.run_lab(TEST_FAIL="chat_drive_htree_product")
        self.assertEqual(result.returncode, 23, result.stderr)
        self.assertFalse((self.install / "receipt.json").exists())
        self.assertNotIn("relayless_mesh_product", self.commands()[-1])

    def test_missing_metrics_never_writes_success(self):
        result = self.run_lab(TEST_NO_METRICS="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.install / "receipt.json").exists())

    def test_malformed_metrics_never_preserve_success(self):
        self.install.mkdir()
        for metrics in ["broken JSON", "[]", '{"idle":[{"cpu_percent":[]}]}']:
            with self.subTest(metrics=metrics):
                (self.install / "receipt.json").write_text('{"status":"passed"}')
                result = self.run_lab(TEST_BAD_METRICS=metrics)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((self.install / "receipt.json").exists())

    def test_local_binary_is_recorded_without_claiming_public_source(self):
        binary = self.executable("local-htree", "#!/bin/sh\nexit 0\n")
        result = self.run_lab(IRIS_STACK_HTREE_BIN=str(binary))
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads((self.install / "receipt.json").read_text())
        self.assertEqual(receipt["products"]["hashtree"]["source"], "local-binary")
        self.assertNotIn("version", receipt["products"]["hashtree"])

    def test_release_mode_rejects_binary_and_repository_overrides(self):
        binary = self.executable("local-htree", "#!/bin/sh\nexit 0\n")
        for overrides in [dict(IRIS_STACK_HTREE_BIN=str(binary)),
                          dict(IRIS_STACK_CHAT_GIT="file:///tmp/source")]:
            with self.subTest(overrides=overrides):
                result = self.run_lab(IRIS_STACK_RELEASE_GATE="1", **overrides)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.calls.exists())

    def test_release_mode_binds_clean_lab_and_fixed_cpu_budget(self):
        lab = self.root / "lab"
        (lab / "scripts").mkdir(parents=True)
        for name in ["product-lab.sh", "product-lab.py"]:
            shutil.copy2(ROOT / "scripts" / name, lab / "scripts" / name)
        subprocess.run(["git", "init", "-q", str(lab)], check=True)
        subprocess.run(["git", "add", "scripts"], cwd=lab, check=True)
        subprocess.run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                        "commit", "-qm", "Fixture source"], cwd=lab, check=True)
        rev = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=lab, text=True).strip()
        env = {**self.env, "IRIS_STACK_RELEASE_GATE": "1", "IRIS_STACK_LAB_REV": rev,
               "IRIS_STACK_IDLE_MAX_CPU_PERCENT": "100", "IRIS_STACK_REQUIRE_CPU": "0"}
        command = ["sh", str(lab / "scripts/product-lab.sh")]
        result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads((self.install / "receipt.json").read_text())
        self.assertEqual(receipt["lab_revision"], rev)
        self.assertTrue(receipt["release_gate"])
        self.assertTrue(receipt["lab_worktree_clean"])
        self.assertEqual(json.loads((self.root / "runtime-env.json").read_text()),
                         {"IRIS_STACK_REQUIRE_CPU": "1", "IRIS_STACK_IDLE_MAX_CPU_PERCENT": "5"})
        result = subprocess.run(command, env={**env, "IRIS_STACK_LAB_REV": ""},
                                capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.install / "receipt.json").exists())
        untracked = lab / "scripts/untracked-replacement.py"
        untracked.write_text("# unexpected source\n")
        result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.install / "receipt.json").exists())
        untracked.unlink()
        for dirty in [False, True]:
            if dirty:
                with (lab / "scripts/product-lab.py").open("a") as source:
                    source.write("\n# changed after pin\n")
            result = subprocess.run(command, env={**env, "IRIS_STACK_LAB_REV": rev if dirty else SHA},
                                    capture_output=True, text=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((self.install / "receipt.json").exists())


if __name__ == "__main__":
    unittest.main()
