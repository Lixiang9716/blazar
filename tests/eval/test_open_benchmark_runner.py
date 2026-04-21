import importlib.util
import sys
import unittest
from pathlib import Path


def load_module():
    repo_root = Path(__file__).resolve().parents[2]
    script_path = repo_root / "scripts" / "eval" / "open_benchmark_runner.py"
    spec = importlib.util.spec_from_file_location("open_benchmark_runner", script_path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class BenchmarkRunnerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.mod = load_module()

    def test_select_first_text_field(self):
        row = {"question": "  hello world  ", "query": "fallback"}
        value = self.mod.select_first_text_field(row, ("question", "query"))
        self.assertEqual(value, "hello world")

    def test_build_case_returns_none_when_prompt_missing(self):
        cfg = self.mod.DATASET_REGISTRY["bfcl"]
        case = self.mod.build_case("bfcl", 0, {"function": "x"}, cfg)
        self.assertIsNone(case)

    def test_evaluate_exact_match_normalizes_whitespace(self):
        self.assertTrue(self.mod.evaluate_exact_match("Hello   World", "hello world"))
        self.assertIsNone(self.mod.evaluate_exact_match("", "hello world"))

    def test_build_eval_prompt_applies_max_chars(self):
        prompt = self.mod.build_eval_prompt("swebench_lite", "abcdef", max_prompt_chars=3)
        self.assertIn("Task:\nabc", prompt)


if __name__ == "__main__":
    unittest.main()
