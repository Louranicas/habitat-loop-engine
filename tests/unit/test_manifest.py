import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class ScaffoldManifestTests(unittest.TestCase):
    def test_plan_toml_is_lowercase_only(self):
        self.assertTrue((ROOT / "plan.toml").exists())
        self.assertFalse((ROOT / "PLAN.toml").exists())

    def test_s01_s13_specs_exist(self):
        specs = sorted((ROOT / "ai_specs").glob("S*.md"))
        self.assertEqual(len(specs), 13)
        self.assertTrue(specs[0].name.startswith("S01_"))
        self.assertTrue(specs[-1].name.startswith("S13_"))

    def test_seven_layer_docs_exist(self):
        layers = sorted((ROOT / "ai_docs" / "layers").glob("L*.md"))
        self.assertEqual(len(layers), 7)

    def test_scaffold_receipt_exists(self):
        receipts = list((ROOT / ".deployment-work" / "receipts").glob("scaffold-authorization-*.md"))
        self.assertTrue(receipts)


if __name__ == "__main__":
    unittest.main()
