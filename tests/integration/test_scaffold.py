import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class ScaffoldIntegrationTests(unittest.TestCase):
    def test_verify_sync_script_passes(self):
        result = subprocess.run(["scripts/verify-sync.sh"], cwd=ROOT, text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_negative_controls_script_passes(self):
        result = subprocess.run(["scripts/verify-negative-controls.sh"], cwd=ROOT, text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_watcher_hardening_scripts_pass(self):
        scripts = [
            "scripts/verify-skeleton-only.sh",
            "scripts/verify-framework-hash-freshness.sh",
            "scripts/verify-vault-parity.sh",
            "scripts/verify-bin-wrapper-parity.sh",
        ]
        for script in scripts:
            with self.subTest(script=script):
                result = subprocess.run([script], cwd=ROOT, text=True, capture_output=True, check=False)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
