#!/usr/bin/env python3
"""Deploy and run open benchmark datasets for model capability testing.

Default deployed datasets:
- bfcl        -> gorilla-llm/Berkeley-Function-Calling-Leaderboard
- toolbench   -> Maurus/ToolBench
- swebench_lite -> princeton-nlp/SWE-bench_Lite
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import os
import statistics
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

DATASETS_SERVER_BASE = "https://datasets-server.huggingface.co"


@dataclasses.dataclass(frozen=True)
class DatasetConfig:
    key: str
    hf_dataset: str
    config: str
    split: str
    prompt_fields: tuple[str, ...]
    answer_fields: tuple[str, ...]
    smoke_rows: int
    prefer_first_rows: bool = False
    max_prompt_chars: int | None = None


DATASET_REGISTRY: dict[str, DatasetConfig] = {
    "bfcl": DatasetConfig(
        key="bfcl",
        hf_dataset="gorilla-llm/Berkeley-Function-Calling-Leaderboard",
        config="default",
        split="train",
        prompt_fields=("question", "query", "problem_statement"),
        answer_fields=("function", "answer"),
        smoke_rows=25,
    ),
    "toolbench": DatasetConfig(
        key="toolbench",
        hf_dataset="Maurus/ToolBench",
        config="default",
        split="train",
        prompt_fields=("query", "question"),
        answer_fields=("api_list",),
        smoke_rows=8,
        prefer_first_rows=True,
    ),
    "swebench_lite": DatasetConfig(
        key="swebench_lite",
        hf_dataset="princeton-nlp/SWE-bench_Lite",
        config="default",
        split="test",
        prompt_fields=("problem_statement",),
        answer_fields=("patch",),
        smoke_rows=25,
        max_prompt_chars=1500,
    ),
}


def parse_dataset_keys(value: str) -> list[str]:
    keys = [item.strip() for item in value.split(",") if item.strip()]
    invalid = [item for item in keys if item not in DATASET_REGISTRY]
    if invalid:
        raise ValueError(
            f"unknown dataset key(s): {', '.join(invalid)}. "
            f"valid keys: {', '.join(sorted(DATASET_REGISTRY))}"
        )
    return keys


def select_first_text_field(row: dict[str, Any], fields: tuple[str, ...]) -> str:
    for field in fields:
        value = row.get(field)
        if isinstance(value, str) and value.strip():
            return value.strip()
    return ""


def normalize_text(value: str) -> str:
    return " ".join(value.strip().lower().split())


def tokenize(text: str) -> list[str]:
    """Simple whitespace + punctuation tokenizer for scoring."""
    return normalize_text(text).split()


def keyword_recall(reference: str, prediction: str) -> float | None:
    """Fraction of reference tokens found in prediction. None if reference empty."""
    ref_tokens = set(tokenize(reference))
    if not ref_tokens:
        return None
    pred_tokens = set(tokenize(prediction))
    hits = ref_tokens & pred_tokens
    return round(len(hits) / len(ref_tokens), 4)


def ngram_similarity(reference: str, prediction: str, n: int = 2) -> float | None:
    """Unigram+bigram overlap (F1-like). None if reference empty."""
    ref_tokens = tokenize(reference)
    pred_tokens = tokenize(prediction)
    if not ref_tokens or not pred_tokens:
        return None

    def _ngrams(tokens: list[str], size: int) -> list[str]:
        return [" ".join(tokens[i : i + size]) for i in range(len(tokens) - size + 1)]

    scores = []
    for size in range(1, n + 1):
        ref_ng = _ngrams(ref_tokens, size)
        pred_ng = _ngrams(pred_tokens, size)
        if not ref_ng:
            continue
        ref_set = {}
        for g in ref_ng:
            ref_set[g] = ref_set.get(g, 0) + 1
        pred_set = {}
        for g in pred_ng:
            pred_set[g] = pred_set.get(g, 0) + 1
        overlap = sum(min(ref_set.get(g, 0), pred_set.get(g, 0)) for g in ref_set)
        precision = overlap / len(pred_ng) if pred_ng else 0.0
        recall = overlap / len(ref_ng) if ref_ng else 0.0
        if precision + recall > 0:
            scores.append(2 * precision * recall / (precision + recall))
        else:
            scores.append(0.0)
    return round(sum(scores) / len(scores), 4) if scores else None


def compute_adaptive_timeout(prompt_len: int, base_timeout: int) -> int:
    """Scale timeout with prompt length. Long prompts get more time."""
    if prompt_len <= 200:
        return base_timeout
    extra_chunks = (prompt_len - 200) // 500
    return base_timeout + extra_chunks * 60


def format_case_progress(dataset: str, current: int, total: int, case_id: str) -> str:
    return f"[run][{dataset}][{current}/{total}][{case_id}]"


def request_json(
    url: str,
    payload: dict[str, Any] | None = None,
    headers: dict[str, str] | None = None,
    timeout_secs: int = 60,
) -> dict[str, Any]:
    data = None
    req_headers = {"Content-Type": "application/json"}
    if headers:
        req_headers.update(headers)
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers=req_headers)
    with urllib.request.urlopen(req, timeout=timeout_secs) as resp:
        return json.loads(resp.read().decode("utf-8"))


def fetch_rows(dataset: DatasetConfig, offset: int, length: int) -> list[dict[str, Any]]:
    def _first_rows() -> list[dict[str, Any]]:
        first_query = urllib.parse.urlencode(
            {
                "dataset": dataset.hf_dataset,
                "config": dataset.config,
                "split": dataset.split,
            }
        )
        first_url = f"{DATASETS_SERVER_BASE}/first-rows?{first_query}"
        body = request_json(first_url)
        rows = [item.get("row", {}) for item in body.get("rows", [])]
        return rows[offset : offset + length]

    if dataset.prefer_first_rows:
        return _first_rows()

    query = urllib.parse.urlencode(
        {
            "dataset": dataset.hf_dataset,
            "config": dataset.config,
            "split": dataset.split,
            "offset": offset,
            "length": length,
        }
    )
    url = f"{DATASETS_SERVER_BASE}/rows?{query}"
    try:
        body = request_json(url)
    except urllib.error.HTTPError:
        return _first_rows()
    return [item.get("row", {}) for item in body.get("rows", [])]


def build_case(dataset_key: str, row_idx: int, row: dict[str, Any], cfg: DatasetConfig) -> dict[str, Any] | None:
    prompt = select_first_text_field(row, cfg.prompt_fields)
    if not prompt:
        return None
    reference = select_first_text_field(row, cfg.answer_fields)
    case_id = str(row.get("instance_id") or row.get("query_id") or f"{dataset_key}-{row_idx}")
    return {
        "dataset": dataset_key,
        "id": case_id,
        "prompt": prompt,
        "reference": reference,
        "meta": {
            "source_split": cfg.split,
            "source_dataset": cfg.hf_dataset,
        },
    }


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as fh:
        for row in rows:
            fh.write(json.dumps(row, ensure_ascii=False))
            fh.write("\n")


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def command_prepare(args: argparse.Namespace) -> int:
    dataset_keys = parse_dataset_keys(args.datasets)
    mode = args.mode
    summary: dict[str, Any] = {"mode": mode, "datasets": {}}

    verbose = not args.quiet

    for key in dataset_keys:
        cfg = DATASET_REGISTRY[key]
        length = cfg.smoke_rows if mode == "smoke" else args.full_rows
        if verbose:
            print(
                f"[prepare] dataset={key} source={cfg.hf_dataset} split={cfg.split} "
                f"target_rows={length}"
            )
        raw_rows = fetch_rows(cfg, 0, length)
        cases = []
        skipped = 0
        for idx, row in enumerate(raw_rows):
            case = build_case(key, idx, row, cfg)
            if case is None:
                skipped += 1
                continue
            cases.append(case)

        out_path = Path(args.output_dir) / key / f"{mode}.jsonl"
        write_jsonl(out_path, cases)
        summary["datasets"][key] = {
            "fetched_rows": len(raw_rows),
            "usable_cases": len(cases),
            "skipped_rows": skipped,
            "file": str(out_path),
        }
        print(f"[prepare] {key}: fetched={len(raw_rows)} usable={len(cases)} skipped={skipped}")

    summary_path = Path(args.output_dir) / f"prepare_summary_{mode}.json"
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"[prepare] summary written: {summary_path}")
    return 0


def load_provider_config(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def resolve_provider_credentials(args: argparse.Namespace) -> tuple[str, str, str]:
    cfg = load_provider_config(Path(args.provider_config))
    api_key = args.api_key or os.environ.get("BLAZAR_EVAL_API_KEY") or cfg.get("api_key")
    base_url = (
        args.base_url
        or os.environ.get("BLAZAR_EVAL_BASE_URL")
        or cfg.get("base_url")
        or "https://api.siliconflow.cn/v1"
    )
    model = (
        args.model
        or os.environ.get("BLAZAR_EVAL_MODEL")
        or cfg.get("model")
        or "Qwen/Qwen3-8B"
    )
    if not api_key and not args.dry_run:
        raise ValueError("missing API key. Set --api-key or BLAZAR_EVAL_API_KEY.")
    if not base_url:
        raise ValueError("missing base URL. Set --base-url or BLAZAR_EVAL_BASE_URL.")
    if not model:
        raise ValueError("missing model. Set --model or BLAZAR_EVAL_MODEL.")
    return api_key or "", base_url.rstrip("/"), model


def build_eval_prompt(dataset: str, prompt: str, max_prompt_chars: int | None) -> str:
    if max_prompt_chars is not None and len(prompt) > max_prompt_chars:
        prompt = prompt[:max_prompt_chars]
    return (
        f"You are being evaluated on {dataset}. "
        "Solve the task directly and provide a concise final answer.\n\n"
        f"Task:\n{prompt}"
    )


def call_openai_compatible_chat(
    base_url: str,
    api_key: str,
    model: str,
    prompt: str,
    timeout_secs: int,
) -> tuple[str, int]:
    url = f"{base_url}/chat/completions"
    payload = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": False,
        "temperature": 0.0,
    }
    headers = {"Authorization": f"Bearer {api_key}"}
    start = time.perf_counter()
    body = request_json(url, payload=payload, headers=headers, timeout_secs=timeout_secs)
    elapsed_ms = int((time.perf_counter() - start) * 1000)
    choices = body.get("choices") or []
    if not choices:
        raise RuntimeError("no choices returned from provider")
    message = choices[0].get("message") or {}
    content = message.get("content")
    if not isinstance(content, str):
        raise RuntimeError("provider returned non-text content")
    return content, elapsed_ms


def evaluate_exact_match(reference: str, prediction: str) -> bool | None:
    if not reference.strip():
        return None
    if len(reference) > 200 or "\n" in reference:
        return None
    return normalize_text(reference) == normalize_text(prediction)


def evaluate_multi_tier(reference: str, prediction: str) -> dict[str, Any]:
    """Multi-tier scoring: exact_match, keyword_recall, ngram_similarity."""
    result: dict[str, Any] = {
        "exact_match": evaluate_exact_match(reference, prediction),
        "keyword_recall": keyword_recall(reference, prediction),
        "ngram_f1": ngram_similarity(reference, prediction, n=2),
    }
    return result


def command_run(args: argparse.Namespace) -> int:
    dataset_keys = parse_dataset_keys(args.datasets)
    api_key, base_url, model = resolve_provider_credentials(args)
    report: dict[str, Any] = {
        "mode": args.mode,
        "model": model,
        "base_url": base_url,
        "datasets": {},
    }

    verbose = not args.quiet

    for key in dataset_keys:
        cfg = DATASET_REGISTRY[key]
        in_path = Path(args.input_dir) / key / f"{args.mode}.jsonl"
        if not in_path.exists():
            raise FileNotFoundError(
                f"prepared dataset file not found: {in_path}. run `prepare` first."
            )
        cases = read_jsonl(in_path)[: args.max_cases]
        if verbose:
            print(
                f"[run] dataset={key} mode={args.mode} cases={len(cases)} "
                f"model={model} timeout={args.timeout_secs}s retries={args.retries}"
            )
        outputs = []
        latencies = []
        exact_match_hits = 0
        exact_match_total = 0
        keyword_recall_sum = 0.0
        keyword_recall_count = 0
        ngram_f1_sum = 0.0
        ngram_f1_count = 0
        success = 0

        for idx, case in enumerate(cases, start=1):
            prompt = build_eval_prompt(key, case["prompt"], cfg.max_prompt_chars)
            prefix = format_case_progress(key, idx, len(cases), str(case["id"]))
            case_timeout = compute_adaptive_timeout(len(case["prompt"]), args.timeout_secs)
            row = {
                "dataset": key,
                "id": case["id"],
                "reference": case.get("reference", ""),
                "prompt_len": len(case["prompt"]),
            }
            try:
                if verbose:
                    print(f"{prefix} start prompt_len={len(case['prompt'])} timeout={case_timeout}s")
                if args.dry_run:
                    prediction = "[dry-run] skipped provider call"
                    latency_ms = 0
                    if verbose:
                        print(f"{prefix} dry-run skip provider call")
                else:
                    prediction = ""
                    latency_ms = 0
                    for attempt in range(args.retries + 1):
                        # Escalate timeout on retries (1.5× per retry)
                        attempt_timeout = int(case_timeout * (1.5 ** attempt))
                        try:
                            if verbose:
                                print(
                                    f"{prefix} attempt={attempt + 1}/{args.retries + 1} "
                                    f"timeout={attempt_timeout}s"
                                )
                            prediction, latency_ms = call_openai_compatible_chat(
                                base_url, api_key, model, prompt, attempt_timeout
                            )
                            if verbose:
                                print(f"{prefix} success latency_ms={latency_ms}")
                            break
                        except Exception as exc:
                            if verbose:
                                print(f"{prefix} error attempt={attempt + 1}: {exc}")
                            if attempt == args.retries:
                                raise
                row["prediction"] = prediction
                row["latency_ms"] = latency_ms
                row["error"] = None
                success += 1
                latencies.append(latency_ms)
                # Multi-tier scoring
                scores = evaluate_multi_tier(case.get("reference", ""), prediction)
                row["exact_match"] = scores["exact_match"]
                row["keyword_recall"] = scores["keyword_recall"]
                row["ngram_f1"] = scores["ngram_f1"]
                if scores["exact_match"] is not None:
                    exact_match_total += 1
                    if scores["exact_match"]:
                        exact_match_hits += 1
                if scores["keyword_recall"] is not None:
                    keyword_recall_sum += scores["keyword_recall"]
                    keyword_recall_count += 1
                if scores["ngram_f1"] is not None:
                    ngram_f1_sum += scores["ngram_f1"]
                    ngram_f1_count += 1
                if verbose and (scores["keyword_recall"] is not None or scores["ngram_f1"] is not None):
                    print(
                        f"{prefix} scores keyword_recall={scores['keyword_recall']} "
                        f"ngram_f1={scores['ngram_f1']}"
                    )
            except Exception as exc:  # explicit reporting to benchmark logs
                row["prediction"] = ""
                row["latency_ms"] = None
                row["error"] = str(exc)
                row["exact_match"] = None
                row["keyword_recall"] = None
                row["ngram_f1"] = None
                if verbose:
                    print(f"{prefix} failed final_error={exc}")
            outputs.append(row)

        out_path = Path(args.report_dir) / f"{key}_{args.mode}_predictions.jsonl"
        write_jsonl(out_path, outputs)
        failure = len(cases) - success
        report["datasets"][key] = {
            "total_cases": len(cases),
            "success": success,
            "failure": failure,
            "success_rate": round(success / len(cases), 4) if cases else 0.0,
            "exact_match_total": exact_match_total,
            "exact_match_hits": exact_match_hits,
            "exact_match_rate": (
                round(exact_match_hits / exact_match_total, 4)
                if exact_match_total
                else None
            ),
            "keyword_recall_avg": (
                round(keyword_recall_sum / keyword_recall_count, 4)
                if keyword_recall_count
                else None
            ),
            "ngram_f1_avg": (
                round(ngram_f1_sum / ngram_f1_count, 4)
                if ngram_f1_count
                else None
            ),
            "scored_cases": max(keyword_recall_count, ngram_f1_count),
            "latency_ms_p50": int(statistics.median(latencies)) if latencies else None,
            "latency_ms_p95": (
                int(sorted(latencies)[max(0, int(0.95 * len(latencies)) - 1)])
                if latencies
                else None
            ),
            "predictions_file": str(out_path),
        }
        scored_summary = ""
        if keyword_recall_count:
            kr = round(keyword_recall_sum / keyword_recall_count, 4)
            scored_summary += f" keyword_recall={kr}"
        if ngram_f1_count:
            nf = round(ngram_f1_sum / ngram_f1_count, 4)
            scored_summary += f" ngram_f1={nf}"
        print(
            f"[run] {key}: total={len(cases)} success={success} "
            f"failure={failure} exact_match={exact_match_hits}/{exact_match_total}"
            f"{scored_summary}"
        )
        if verbose:
            print(f"[run] {key} predictions: {out_path}")

    report_path = Path(args.report_dir) / f"report_{args.mode}.json"
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"[run] report written: {report_path}")
    return 0


def command_self_test(_: argparse.Namespace) -> int:
    row = {"question": "  test prompt ", "function": " answer "}
    case = build_case("bfcl", 0, row, DATASET_REGISTRY["bfcl"])
    assert case is not None
    assert case["prompt"] == "test prompt"
    assert evaluate_exact_match("Hello   World", "hello world") is True
    assert evaluate_exact_match("", "anything") is None
    print("[self-test] passed")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Open benchmark deployment and automation")
    subparsers = parser.add_subparsers(dest="command", required=True)

    prepare = subparsers.add_parser("prepare", help="Download/prepare benchmark samples")
    prepare.add_argument("--datasets", default="bfcl,toolbench,swebench_lite")
    prepare.add_argument("--mode", choices=("smoke", "full"), default="smoke")
    prepare.add_argument("--full-rows", type=int, default=250)
    prepare.add_argument("--output-dir", default="target/evals/datasets")
    prepare.add_argument("--quiet", action="store_true")
    prepare.set_defaults(func=command_prepare)

    run = subparsers.add_parser("run", help="Run model against prepared benchmarks")
    run.add_argument("--datasets", default="bfcl,toolbench,swebench_lite")
    run.add_argument("--mode", choices=("smoke", "full"), default="smoke")
    run.add_argument("--input-dir", default="target/evals/datasets")
    run.add_argument("--report-dir", default="target/evals/reports")
    run.add_argument("--provider-config", default="config/provider.json")
    run.add_argument("--api-key", default=None)
    run.add_argument("--base-url", default=None)
    run.add_argument("--model", default=None)
    run.add_argument("--max-cases", type=int, default=20)
    run.add_argument("--timeout-secs", type=int, default=120)
    run.add_argument("--retries", type=int, default=2)
    run.add_argument("--dry-run", action="store_true")
    run.add_argument("--quiet", action="store_true")
    run.set_defaults(func=command_run)

    self_test = subparsers.add_parser("self-test", help="Run built-in harness checks")
    self_test.set_defaults(func=command_self_test)

    return parser


def main(argv: list[str]) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
