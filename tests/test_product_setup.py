"""Pure workflow/setup contracts; all external commands are mocked."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("product_setup", ROOT / "scripts/setup-product-inputs.py")
setup = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(setup)


class WorkflowContract(unittest.TestCase):
    def test_both_jobs_use_exact_reviewed_setup_outside_clean_lab(self):
        workflow = (ROOT / ".github/workflows/product-lab.yml").read_text()
        jobs = dict(re.findall(r"(?ms)^  ([\w-]+):\n(.*?)(?=^  [\w-]+:\n|\Z)",
                               workflow.split("\njobs:\n", 1)[1]))
        self.assertEqual(set(jobs), {"cashu-service-payment", "chat-drive-hashtree"})
        self.assertIn("defaults:\n  run:\n    working-directory: iris-stack\n", workflow)
        self.assertEqual(workflow.count("      - scripts/setup-product-inputs.py\n"), 2)
        self.assertEqual(workflow.count("      - tests/test_product_setup.py\n"), 2)
        for name, job in jobs.items():
            with self.subTest(job=name):
                self.assertIn("runs-on: ubuntu-24.04", job)
                self.assertIn("timeout-minutes: " + ("15" if name == "cashu-service-payment" else "45"), job)
                self.assertIn("repository: irislib/iris-stack", job)
                self.assertIn("path: iris-stack\n", job)
                self.assertIn("ref: ${{ env.IRIS_STACK_LAB_REV }}", job)
                self.assertIn("repository: irislib/iris-chat-rs", job)
                self.assertIn("ref: " + setup.SOURCE, job)
                self.assertIn("path: iris-build-inputs\n", job)
                self.assertEqual(job.count("persist-credentials: false"), 2)
                self.assertIn("sparse-checkout-cone-mode: false", job)
                for path in setup.PINS:
                    self.assertIn("            /" + path + "\n", job)
                self.assertIn('python3 scripts/setup-product-inputs.py --inputs "$GITHUB_WORKSPACE/iris-build-inputs" --work "$RUNNER_TEMP/iris-product-inputs"', job)
                self.assertLess(job.index("Check pinned setup contracts"), job.index("Install verified product build inputs"))
                self.assertLess(job.index("Install verified product build inputs"), job.index("      - name: Run "))
                self.assertIn("name: iris-stack-build-inputs-${{ github.job }}", job)
                self.assertIn("if: ${{ always() }}", job)
                self.assertIn("${{ runner.temp }}/iris-product-inputs/setup.json", job)
                self.assertIn("${{ runner.temp }}/iris-product-inputs/rust/receipt.json", job)
                self.assertNotIn("apt-get", job)
                self.assertNotIn("rust-toolchain@", job)
                for action in re.findall(r"uses: (\S+)", job):
                    self.assertRegex(action, r"^[\w./-]+@[0-9a-f]{40}$")
        self.assertIn("cargo test --locked --test cashu_payment_product -- --nocapture", jobs["cashu-service-payment"])
        product = jobs["chat-drive-hashtree"]
        self.assertIn("IRIS_STACK_RELEASE_GATE: '1'", product)
        for key in ("DRIVE_REV", "HTREE_VERSION", "HTREE_REV", "CHAT_REV"):
            self.assertIn("IRIS_STACK_" + key + ": ${{ inputs.", product)
        self.assertIn("run: scripts/product-lab.sh", product)
        for filename in ("receipt.json", "product-inputs.json", "mesh-metrics.json"):
            self.assertIn("iris-stack/target/product-lab/" + filename, product)
        self.assertIn("name: iris-stack-product-gate", product)
        self.assertIn("name: iris-stack-product-diagnostics", product)


    def test_native_ci_reuses_same_setup_and_preserves_all_target_tests(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        jobs = dict(re.findall(r"(?ms)^  ([\w-]+):\n(.*?)(?=^  [\w-]+:\n|\Z)",
                               workflow.split("\njobs:\n", 1)[1]))
        self.assertEqual(set(jobs), {"test"})
        job = jobs["test"]
        self.assertIn("working-directory: iris-stack", workflow)
        self.assertIn("runs-on: ubuntu-24.04", job)
        self.assertIn("timeout-minutes: 30", job)
        self.assertIn("path: iris-stack\n", job)
        self.assertIn("path: iris-build-inputs\n", job)
        self.assertIn("repository: irislib/iris-chat-rs", job)
        self.assertIn("ref: " + setup.SOURCE, job)
        self.assertEqual(job.count("persist-credentials: false"), 2)
        self.assertIn("sparse-checkout-cone-mode: false", job)
        for path in setup.PINS:
            self.assertIn("            /" + path + "\n", job)
        self.assertIn('python3 scripts/setup-product-inputs.py --inputs "$GITHUB_WORKSPACE/iris-build-inputs" --work "$RUNNER_TEMP/iris-product-inputs"', job)
        self.assertLess(job.index("Install verified product build inputs"),
                        job.index("Run native process and protocol tests"))
        self.assertIn("name: iris-stack-build-inputs-${{ github.job }}", job)
        self.assertIn("if: ${{ always() }}", job)
        self.assertIn("${{ runner.temp }}/iris-product-inputs/setup.json", job)
        self.assertIn("${{ runner.temp }}/iris-product-inputs/rust/receipt.json", job)
        self.assertNotIn("apt-get", job)
        self.assertNotIn("rust-toolchain@", job)
        for action in re.findall(r"uses: (\S+)", job):
            self.assertRegex(action, r"^[\w./-]+@[0-9a-f]{40}$")
        checks = job.split("      - name: Run native process and protocol tests\n", 1)[1]
        self.assertIn("python3 -B tests/test_product_setup.py", checks)
        self.assertIn("python3 tests/test_product_lab.py", checks)
        self.assertIn("cargo test --locked --all-targets", checks)
        self.assertLess(checks.index("python3 -B tests/test_product_setup.py"), checks.index("cargo test"))


class SetupContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        root = Path(self.temporary.name)
        self.inputs, self.work = root / "inputs", root / "work"
        self.inputs.mkdir()
        pins = {}
        for name in setup.PINS:
            path = self.inputs / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("synthetic setup input: " + name)
            pins[name] = hashlib.sha256(path.read_bytes()).hexdigest()
        self.source = setup.SOURCE
        self.dirty = ""
        self.fail_command = None
        self.bad_export = False
        self.bad_path = False
        self.bad_release = False
        self.calls = []
        self.environment_file, self.path_file = root / "env", root / "path"
        self.environment_file.write_text("")
        self.path_file.write_text("")
        self.env = {"GITHUB_ACTIONS": "true", "GITHUB_ENV": str(self.environment_file),
                    "GITHUB_PATH": str(self.path_file), "GITHUB_SHA": "a" * 40}
        for context in (patch.dict(os.environ, self.env), patch.object(setup, "PINS", pins),
                        patch.object(setup.platform, "system", return_value="Linux"),
                        patch.object(setup.platform, "machine", return_value="x86_64"),
                        patch.object(setup.subprocess, "check_output", side_effect=self.output),
                        patch.object(setup.subprocess, "run", side_effect=self.run_command)):
            context.start()
            self.addCleanup(context.stop)
        read_text = Path.read_text
        def read(path, *args, **kwargs):
            if str(path) == "/etc/os-release":
                return 'ID=ubuntu\nVERSION_ID="24.04"\n'
            return read_text(path, *args, **kwargs)
        context = patch.object(Path, "read_text", read)
        context.start()
        self.addCleanup(context.stop)

    def output(self, command, **kwargs):
        self.assertEqual(kwargs, {"text": True, "timeout": 30})
        self.calls.append(command)
        if command[0] == "git":
            return self.source if command[3] == "rev-parse" else self.dirty
        if command[0] == "dpkg-query":
            return "1.2.3"
        if command[0] in ("/usr/bin/pkg-config", "/usr/bin/protoc"):
            return "synthetic version"
        self.fail("Unexpected external command: " + repr(command))

    def run_command(self, command, **kwargs):
        self.assertEqual(kwargs, {"check": True})
        self.calls.append(command)
        if command[0] == self.fail_command:
            raise subprocess.CalledProcessError(17, command)
        if command[0] == "python3":
            prefix = self.work / "rust/toolchain/bin"
            prefix.mkdir(parents=True)
            for name in ("cargo", "rustc", "rustdoc"):
                binary = prefix / name
                binary.write_text("synthetic executable placeholder")
                binary.chmod(0o700)
                export = "/usr/bin/" + name if self.bad_export else str(binary)
                with self.environment_file.open("a") as stream:
                    stream.write(name.upper() + "=" + export + "\n")
            self.path_file.write_text(("/usr/bin" if self.bad_path else str(prefix)) + "\n")
            receipt = {"passed": True, "sourceCommit": setup.SOURCE,
                       "host": "x86_64-unknown-linux-gnu", "versions": {
                           "RUSTC": "release: " + ("1.98.0" if self.bad_release else "1.99.0"),
                           "CARGO": "cargo 1.99.0 synthetic"}}
            (self.work / "rust/receipt.json").write_text(json.dumps(receipt))
        else:
            self.assertEqual(command, ["sudo", "bash", str(self.inputs / "linux/scripts/pinned-apt.sh"),
                                       "ubuntu-noble", "libdbus-1-dev", "pkg-config", "protobuf-compiler"])
        return subprocess.CompletedProcess(command, 0)

    def receipt(self):
        return json.loads((self.work / "setup.json").read_text())

    def test_success_invokes_existing_installers_and_retains_source_receipts(self):
        setup.setup(self.inputs, self.work)
        installs = [command for command in self.calls if command[0] in ("python3", "sudo")]
        self.assertEqual(installs[0], ["python3", str(self.inputs / "scripts/release-build-inputs.py"),
                                     "--install", "--work", str(self.work / "rust")])
        self.assertEqual(len(installs), 2)
        result = self.receipt()
        self.assertTrue(result["passed"])
        self.assertEqual(result["sourceCommit"], setup.SOURCE)
        self.assertEqual(result["inputSHA256"], setup.PINS)
        self.assertEqual(set(result["packages"]), set(setup.PACKAGES))
        self.assertEqual(result["compilerReceiptSHA256"], hashlib.sha256((self.work / "rust/receipt.json").read_bytes()).hexdigest())
        self.assertTrue(self.environment_file.read_text().endswith("PKG_CONFIG=/usr/bin/pkg-config\nPROTOC=/usr/bin/protoc\n"))

    def test_wrong_source_fails_before_install_and_retains_failure(self):
        self.source = "b" * 40
        with self.assertRaisesRegex(ValueError, "reviewed source"):
            setup.setup(self.inputs, self.work)
        self.assertFalse(any(command[0] in ("python3", "sudo") for command in self.calls))
        self.assertFalse(self.receipt()["passed"])

    def test_dirty_source_fails_before_install(self):
        self.dirty = " M scripts/release-build-inputs.py"
        with self.assertRaisesRegex(ValueError, "dirty"):
            setup.setup(self.inputs, self.work)
        self.assertFalse(any(command[0] in ("python3", "sudo") for command in self.calls))
        self.assertFalse(self.receipt()["passed"])

    def test_mutated_input_fails_before_install(self):
        (self.inputs / "scripts/release-build-inputs.json").write_text("wrong input")
        with self.assertRaisesRegex(ValueError, "differs"):
            setup.setup(self.inputs, self.work)
        self.assertFalse(any(command[0] in ("python3", "sudo") for command in self.calls))
        self.assertFalse(self.receipt()["passed"])

    def test_wrong_compiler_export_is_rejected_before_apt(self):
        self.bad_export = True
        with self.assertRaisesRegex(ValueError, "wrong tool path"):
            setup.setup(self.inputs, self.work)
        self.assertFalse(any(command[0] == "sudo" for command in self.calls))
        self.assertFalse(self.receipt()["passed"])

    def test_wrong_path_precedence_is_rejected_before_apt(self):
        self.bad_path = True
        with self.assertRaisesRegex(ValueError, "first PATH"):
            setup.setup(self.inputs, self.work)
        self.assertFalse(any(command[0] == "sudo" for command in self.calls))
        self.assertFalse(self.receipt()["passed"])

    def test_wrong_compiler_release_is_rejected_before_apt(self):
        self.bad_release = True
        with self.assertRaisesRegex(ValueError, "wrong toolchain"):
            setup.setup(self.inputs, self.work)
        self.assertFalse(any(command[0] == "sudo" for command in self.calls))
        self.assertFalse(self.receipt()["passed"])

    def test_compiler_failure_retains_failure_without_apt(self):
        self.fail_command = "python3"
        with self.assertRaises(subprocess.CalledProcessError):
            setup.setup(self.inputs, self.work)
        self.assertFalse(any(command[0] == "sudo" for command in self.calls))
        self.assertFalse(self.receipt()["passed"])

    def test_apt_failure_retains_compiler_receipt_without_native_exports(self):
        self.fail_command = "sudo"
        with self.assertRaises(subprocess.CalledProcessError):
            setup.setup(self.inputs, self.work)
        self.assertFalse(self.receipt()["passed"])
        self.assertIn("compilerReceiptSHA256", self.receipt())
        self.assertNotIn("PKG_CONFIG=", self.environment_file.read_text())

    def test_existing_work_is_preserved(self):
        self.work.mkdir()
        marker = self.work / "setup.json"
        marker.write_text("existing evidence")
        with self.assertRaises(FileExistsError):
            setup.setup(self.inputs, self.work)
        self.assertEqual(marker.read_text(), "existing evidence")
        self.assertFalse(self.calls)

    def test_non_hosted_execution_is_rejected_before_writes(self):
        with patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}):
            with self.assertRaisesRegex(ValueError, "hosted"):
                setup.setup(self.inputs, self.work)
        self.assertFalse(self.work.exists())
        self.assertFalse(self.calls)


if __name__ == "__main__":
    unittest.main()
