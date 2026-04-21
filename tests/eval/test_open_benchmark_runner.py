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

    def test_format_case_progress(self):
        progress = self.mod.format_case_progress("bfcl", 2, 10, "case-123")
        self.assertEqual(progress, "[run][bfcl][2/10][case-123]")

    def test_keyword_recall(self):
        # "the cat sat on the mat" unique tokens: {the, cat, sat, on, mat} = 5
        # "the cat sat" unique tokens: {the, cat, sat} = 3 → recall = 3/5 = 0.6
        self.assertAlmostEqual(
            self.mod.keyword_recall("the cat sat on the mat", "the cat sat"),
            0.6,
            places=1,
        )
        self.assertIsNone(self.mod.keyword_recall("", "anything"))

    def test_ngram_similarity(self):
        score = self.mod.ngram_similarity("the cat sat on the mat", "the cat sat on the mat")
        self.assertEqual(score, 1.0)
        score_diff = self.mod.ngram_similarity("the cat sat", "a dog ran")
        self.assertIsNotNone(score_diff)
        self.assertLess(score_diff, 0.5)
        self.assertIsNone(self.mod.ngram_similarity("", "anything"))

    def test_evaluate_multi_tier(self):
        scores = self.mod.evaluate_multi_tier("hello world", "hello world")
        self.assertTrue(scores["exact_match"])
        self.assertAlmostEqual(scores["keyword_recall"], 1.0)
        self.assertAlmostEqual(scores["ngram_f1"], 1.0)
        # Long ref skips exact match but still computes recall/ngram
        long_ref = "word " * 100
        scores2 = self.mod.evaluate_multi_tier(long_ref, "word word word")
        self.assertIsNone(scores2["exact_match"])
        self.assertIsNotNone(scores2["keyword_recall"])

    def test_compute_adaptive_timeout(self):
        # Short prompt: no increase
        self.assertEqual(self.mod.compute_adaptive_timeout(100, 120), 120)
        # Medium prompt: +60s per 500-char chunk over 200
        self.assertEqual(self.mod.compute_adaptive_timeout(800, 120), 180)
        # Long prompt: +120s
        self.assertEqual(self.mod.compute_adaptive_timeout(1300, 120), 240)


if __name__ == "__main__":
    unittest.main()
