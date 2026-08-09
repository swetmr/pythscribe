# render_report.py — renders the generation-token eval ledger + per-sample
# results into markdown tables. Every cell is traceable: ledger rows carry
# exp_id + raw_ref; per-sample rows live in gen_eval/results/<exp_id>.jsonl.
#
# Usage:
#   python render_report.py                 # all experiments
#   python render_report.py --exp base-001  # one experiment

import argparse
import json
import statistics
from pathlib import Path

HERE = Path(__file__).resolve().parent
LEDGER = HERE / "ledger.jsonl"
RESULTS_DIR = HERE.parent.parent / "gen_eval" / "results"

COND_ORDER = {"python": 0, "ps": 1, "psc": 2}


def load_jsonl(p):
    if not p.exists():
        return []
    return [json.loads(l) for l in p.read_text(encoding="utf-8").splitlines() if l.strip()]


def fmt(x, nd=1):
    if x is None:
        return "—"
    if isinstance(x, float):
        return f"{x:.{nd}f}".rstrip("0").rstrip(".") if nd else f"{x}"
    return str(x)


def render_ledger(rows):
    print("## Ledger aggregates (one row per condition x phase per run)\n")
    print("| exp_id | date | phase | condition | n | o200k out median | IQR | pass rate | syntax-err rate | skill overhead (o200k) | commit | raw |")
    print("|---|---|---|---|---:|---:|---:|---:|---:|---:|---|---|")
    rows = sorted(rows, key=lambda r: (r["exp_id"], r["axis"]["phase"], COND_ORDER.get(r["condition"], 9)))
    for r in rows:
        m = r["metric"]
        commit = (r.get("commit") or "")[:9] or "—"
        print(f"| {r['exp_id']} | {r['date']} | {r['axis']['phase']} | {r['condition']} | {r['n']} "
              f"| {fmt(m['o200k_out_median'])} | {fmt(m['o200k_out_iqr'])} | {fmt(m['pass_rate'], 3)} "
              f"| {fmt(m['syntax_err_rate'], 3)} | {fmt(m['skill_overhead_o200k'], 0)} | {commit} | {r['raw_ref']} |")
    print()


def render_tokens_per_correct(results_by_exp):
    # tokens-per-correct-solution = sum(o200k_out over all samples) / #passing samples.
    # Charges the cost of failed attempts to the condition — the honest accounting
    # for "how many tokens do I spend to get one working program".
    print("## Tokens per correct solution (per experiment x phase x condition)\n")
    print("| exp_id | phase | condition | samples | passes | total o200k out | tokens/correct |")
    print("|---|---|---|---:|---:|---:|---:|")
    for exp_id, rows in sorted(results_by_exp.items()):
        for phase in ("micro", "macro"):
            conds = sorted({r["condition"] for r in rows if r["kind"] == phase}, key=lambda c: COND_ORDER.get(c, 9))
            for cond in conds:
                sub = [r for r in rows if r["kind"] == phase and r["condition"] == cond and r["verdict"] != "model_error"]
                if not sub:
                    continue
                total = sum(r["o200k_out"] or 0 for r in sub)
                passes = sum(1 for r in sub if r["pass"])
                tpc = f"{total / passes:.1f}" if passes else "∞ (0 passes)"
                print(f"| {exp_id} | {phase} | {cond} | {len(sub)} | {passes} | {total} | {tpc} |")
    print()


def render_per_task(results_by_exp):
    print("## Per-task median o200k out (passing samples only; fails in parens)\n")
    for exp_id, rows in sorted(results_by_exp.items()):
        conds = sorted({r["condition"] for r in rows}, key=lambda c: COND_ORDER.get(c, 9))
        tasks = []
        seen = set()
        for r in rows:  # preserve task order of the run
            if r["task"] not in seen:
                seen.add(r["task"])
                tasks.append(r["task"])
        print(f"### {exp_id}\n")
        print("| task | kind | " + " | ".join(conds) + " |")
        print("|---|---|" + "---:|" * len(conds))
        for t in tasks:
            trows = [r for r in rows if r["task"] == t]
            kind = trows[0]["kind"]
            cells = []
            for c in conds:
                sub = [r for r in trows if r["condition"] == c and r["verdict"] != "model_error"]
                if not sub:
                    cells.append("n/a")
                    continue
                ok = [r["o200k_out"] for r in sub if r["pass"] and r["o200k_out"] is not None]
                fails = sum(1 for r in sub if not r["pass"])
                cell = fmt(statistics.median(ok)) if ok else "—"
                if fails:
                    cell += f" ({fails}f)"
                cells.append(cell)
            print(f"| {t} | {kind} | " + " | ".join(cells) + " |")
        print()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exp", default=None, help="restrict to one exp_id")
    args = ap.parse_args()

    ledger = load_jsonl(LEDGER)
    if args.exp:
        ledger = [r for r in ledger if r["exp_id"] == args.exp]
    if not ledger:
        print(f"no ledger rows found in {LEDGER}" + (f" for exp {args.exp}" if args.exp else ""))
        return

    exp_ids = sorted({r["exp_id"] for r in ledger})
    results_by_exp = {}
    for e in exp_ids:
        rows = load_jsonl(RESULTS_DIR / f"{e}.jsonl")
        if rows:
            results_by_exp[e] = rows

    print("# Generation-token eval — rendered report\n")
    print(f"Source ledger: `{LEDGER}` ({len(ledger)} rows, experiments: {', '.join(exp_ids)})\n")
    render_ledger(ledger)
    render_tokens_per_correct(results_by_exp)
    render_per_task(results_by_exp)
    print("Traceability: aggregate rows -> `gen_eval/results/<exp_id>.jsonl` (per-sample "
          "verdicts + token counts) -> `gen_eval/raw/<exp_id>/<task>_<cond>_<n>.md` "
          "(raw completions).")


if __name__ == "__main__":
    main()
