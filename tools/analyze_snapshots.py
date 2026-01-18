#!/usr/bin/env python3
"""
Streaming analyzer for snapshots_YYYY-MM-DD.jsonl (1 JSON object per line).

Goals:
  - Validate JSONL (bad lines, missing fields)
  - Per-mint summary (count, time range, pnl min/max/last, peak_mc max, peak monotonicity violations)
  - Produce a small report you can paste to chat (top winners/losers, suspicious mints)

No external dependencies (stdlib only).
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
from dataclasses import dataclass, asdict
from datetime import datetime, timezone
from typing import Any, Dict, Iterable, Optional, Tuple


def _parse_rfc3339(ts: str) -> Optional[datetime]:
    # Example: "2026-01-17T10:53:14.304197810Z"
    # Python supports up to microseconds; we truncate extra fractional digits.
    if not ts or not isinstance(ts, str):
        return None
    if ts.endswith("Z"):
        ts = ts[:-1] + "+00:00"
    # Truncate nanoseconds to microseconds if present.
    # "....304197810+00:00" -> keep 6 digits
    try:
        if "." in ts:
            head, tail = ts.split(".", 1)
            frac, rest = tail.split("+", 1) if "+" in tail else (tail, "")
            frac = "".join(ch for ch in frac if ch.isdigit())
            if len(frac) > 6:
                frac = frac[:6]
            ts_norm = f"{head}.{frac}+{rest}" if rest else f"{head}.{frac}"
        else:
            ts_norm = ts
        dt = datetime.fromisoformat(ts_norm)
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        return dt.astimezone(timezone.utc)
    except Exception:
        return None


def _is_finite_pos(x: Any) -> bool:
    try:
        v = float(x)
    except Exception:
        return False
    return math.isfinite(v) and v > 0.0


def _to_float(x: Any) -> Optional[float]:
    try:
        v = float(x)
    except Exception:
        return None
    if not math.isfinite(v):
        return None
    return v


@dataclass
class MintStats:
    mint: str
    lines: int = 0
    bad_lines: int = 0

    first_ts: Optional[str] = None
    last_ts: Optional[str] = None

    entry_mc_sol_first: Optional[float] = None
    entry_price_first: Optional[float] = None

    current_mc_sol_last: Optional[float] = None
    current_mc_sol_max: Optional[float] = None
    current_price_last: Optional[float] = None
    pnl_percent_last: Optional[float] = None

    pnl_percent_min: Optional[float] = None
    pnl_percent_max: Optional[float] = None

    peak_mc_sol_max: Optional[float] = None
    peak_mc_sol_decrease_violations: int = 0
    peak_mc_sol_last_seen: Optional[float] = None
    take_profit_crossings: int = 0
    first_take_profit_ts: Optional[str] = None

    time_held_sec_min: Optional[int] = None
    time_held_sec_max: Optional[int] = None

    missing_fields: int = 0

    def ingest(self, obj: Dict[str, Any], take_profit_mc_sol: Optional[float] = None) -> None:
        self.lines += 1

        ts = obj.get("ts")
        if isinstance(ts, str):
            if self.first_ts is None:
                self.first_ts = ts
            self.last_ts = ts

        entry_mc = _to_float(obj.get("entry_mc_sol"))
        entry_price = _to_float(obj.get("entry_price"))
        if self.entry_mc_sol_first is None and entry_mc is not None:
            self.entry_mc_sol_first = entry_mc
        if self.entry_price_first is None and entry_price is not None:
            self.entry_price_first = entry_price

        current_mc = _to_float(obj.get("current_mc_sol"))
        current_price = _to_float(obj.get("current_price"))
        pnl = _to_float(obj.get("pnl_percent"))
        peak_mc = _to_float(obj.get("peak_mc_sol"))

        if current_mc is not None:
            self.current_mc_sol_last = current_mc
            self.current_mc_sol_max = current_mc if self.current_mc_sol_max is None else max(self.current_mc_sol_max, current_mc)
        if current_price is not None:
            self.current_price_last = current_price
        if pnl is not None:
            self.pnl_percent_last = pnl
            self.pnl_percent_min = pnl if self.pnl_percent_min is None else min(self.pnl_percent_min, pnl)
            self.pnl_percent_max = pnl if self.pnl_percent_max is None else max(self.pnl_percent_max, pnl)

        # Peak checks
        if peak_mc is not None:
            self.peak_mc_sol_max = peak_mc if self.peak_mc_sol_max is None else max(self.peak_mc_sol_max, peak_mc)
            if self.peak_mc_sol_last_seen is not None and peak_mc + 1e-12 < self.peak_mc_sol_last_seen:
                self.peak_mc_sol_decrease_violations += 1
            self.peak_mc_sol_last_seen = peak_mc

        # Threshold crossing (take profit)
        if take_profit_mc_sol is not None and current_mc is not None and math.isfinite(take_profit_mc_sol) and take_profit_mc_sol > 0:
            if current_mc >= take_profit_mc_sol:
                self.take_profit_crossings += 1
                if self.first_take_profit_ts is None and isinstance(ts, str):
                    self.first_take_profit_ts = ts

        # time_held
        th = obj.get("time_held_sec")
        if isinstance(th, int):
            self.time_held_sec_min = th if self.time_held_sec_min is None else min(self.time_held_sec_min, th)
            self.time_held_sec_max = th if self.time_held_sec_max is None else max(self.time_held_sec_max, th)

        # Basic required fields check
        required = ("ts", "mint", "entry_mc_sol", "current_mc_sol", "entry_price", "current_price", "pnl_percent", "peak_mc_sol")
        if any(k not in obj for k in required):
            self.missing_fields += 1


def iter_jsonl(path: str) -> Iterable[Tuple[int, Optional[Dict[str, Any]], Optional[str]]]:
    with open(path, "r", encoding="utf-8") as f:
        for i, line in enumerate(f, start=1):
            s = line.strip()
            if not s:
                continue
            try:
                obj = json.loads(s)
                if not isinstance(obj, dict):
                    yield i, None, "not_an_object"
                else:
                    yield i, obj, None
            except json.JSONDecodeError:
                yield i, None, "json_decode_error"


def main(argv: Optional[Iterable[str]] = None) -> int:
    p = argparse.ArgumentParser(description="Analyze snapshots_*.jsonl (JSONL) and print a compact report.")
    p.add_argument("path", help="Path to snapshots_YYYY-MM-DD.jsonl")
    p.add_argument("--mint", default=None, help="Only analyze a single mint (exact match).")
    p.add_argument("--top", type=int, default=10, help="How many top winners/losers to show.")
    p.add_argument("--take-profit-mc-sol", type=float, default=None, help="Check whether current_mc_sol ever crosses this threshold (in SOL).")
    p.add_argument("--take-profit-mc-usd", type=float, default=None, help="Check whether current_mc_sol ever crosses this threshold (in USD). Requires --sol-price-usd.")
    p.add_argument("--sol-price-usd", type=float, default=None, help="SOL price in USD used for USD<->SOL conversion in this report.")
    p.add_argument("--out-json", default=None, help="Write full per-mint stats to a JSON file.")
    p.add_argument("--max-bad-lines", type=int, default=20, help="Show at most N bad line numbers.")
    args = p.parse_args(list(argv) if argv is not None else None)

    path = args.path
    if not os.path.exists(path):
        print(f"ERROR: file not found: {path}", file=sys.stderr)
        return 2

    total_lines = 0
    parsed_objects = 0
    bad_lines: list[Tuple[int, str]] = []
    by_mint: Dict[str, MintStats] = {}

    take_profit_mc_sol: Optional[float] = args.take_profit_mc_sol
    if take_profit_mc_sol is None and args.take_profit_mc_usd is not None:
        if args.sol_price_usd is None or args.sol_price_usd <= 0 or not math.isfinite(args.sol_price_usd):
            print("ERROR: --take-profit-mc-usd requires --sol-price-usd > 0", file=sys.stderr)
            return 2
        take_profit_mc_sol = args.take_profit_mc_usd / args.sol_price_usd

    for line_no, obj, err in iter_jsonl(path):
        total_lines += 1
        if err is not None:
            if len(bad_lines) < args.max_bad_lines:
                bad_lines.append((line_no, err))
            continue

        mint = obj.get("mint")
        if not isinstance(mint, str) or not mint:
            if len(bad_lines) < args.max_bad_lines:
                bad_lines.append((line_no, "missing_or_invalid_mint"))
            continue

        if args.mint is not None and mint != args.mint:
            continue

        parsed_objects += 1
        st = by_mint.get(mint)
        if st is None:
            st = MintStats(mint=mint)
            by_mint[mint] = st
        st.ingest(obj, take_profit_mc_sol=take_profit_mc_sol)

    mints = list(by_mint.values())

    # Ranking by last pnl (if exists)
    def last_pnl(st: MintStats) -> float:
        return st.pnl_percent_last if st.pnl_percent_last is not None else float("-inf")

    winners = sorted((s for s in mints if s.pnl_percent_last is not None), key=last_pnl, reverse=True)[: args.top]
    losers = sorted((s for s in mints if s.pnl_percent_last is not None), key=last_pnl)[: args.top]

    # Peak violations
    peak_bad = sorted((s for s in mints if s.peak_mc_sol_decrease_violations > 0), key=lambda s: s.peak_mc_sol_decrease_violations, reverse=True)

    print("=== snapshots.jsonl report ===")
    print(f"file: {os.path.abspath(path)}")
    print(f"lines_read: {total_lines}")
    print(f"objects_parsed: {parsed_objects}")
    print(f"distinct_mints: {len(mints)}")
    if args.mint:
        print(f"mint_filter: {args.mint}")
    if take_profit_mc_sol is not None:
        if args.take_profit_mc_usd is not None and args.sol_price_usd is not None:
            print(f"take_profit_threshold: {args.take_profit_mc_usd:.6f} USD  (~{take_profit_mc_sol:.6f} SOL at {args.sol_price_usd:.6f} USD/SOL)")
        else:
            print(f"take_profit_threshold: {take_profit_mc_sol:.6f} SOL")

    if bad_lines:
        print("\n-- bad lines (first few) --")
        for ln, why in bad_lines:
            print(f"{ln}: {why}")

    if peak_bad:
        print("\n-- peak_mc_sol decreases detected (should be 0 after fix) --")
        for s in peak_bad[: args.top]:
            print(f"{s.mint}  violations={s.peak_mc_sol_decrease_violations}  lines={s.lines}  last_ts={s.last_ts}")

    if winners:
        print("\n-- top winners by pnl_percent_last --")
        for s in winners:
            print(f"{s.mint}  pnl_last={s.pnl_percent_last:.6f}%  pnl_max={s.pnl_percent_max:.6f}%  lines={s.lines}")

    if losers:
        print("\n-- top losers by pnl_percent_last --")
        for s in losers:
            print(f"{s.mint}  pnl_last={s.pnl_percent_last:.6f}%  pnl_min={s.pnl_percent_min:.6f}%  lines={s.lines}")

    # Profit giveback (pnl_max - pnl_last)
    def giveback(st: MintStats) -> Optional[float]:
        if st.pnl_percent_last is None or st.pnl_percent_max is None:
            return None
        return st.pnl_percent_max - st.pnl_percent_last

    givebacks = [(s, giveback(s)) for s in mints]
    givebacks = [(s, g) for (s, g) in givebacks if g is not None]
    givebacks.sort(key=lambda x: x[1], reverse=True)
    if givebacks:
        print("\n-- top giveback (pnl_max - pnl_last) --")
        for s, g in givebacks[: args.top]:
            print(f"{s.mint}  giveback={g:.6f}%  pnl_max={s.pnl_percent_max:.6f}%  pnl_last={s.pnl_percent_last:.6f}%  peak_mc_max={s.peak_mc_sol_max}  current_mc_max={s.current_mc_sol_max}")

    # Take-profit threshold crossings
    if take_profit_mc_sol is not None:
        crossed = [s for s in mints if s.take_profit_crossings > 0]
        if crossed:
            crossed.sort(key=lambda s: s.take_profit_crossings, reverse=True)
            print("\n-- take profit threshold crossings (current_mc_sol >= threshold) --")
            for s in crossed[: args.top]:
                print(f"{s.mint}  crossings={s.take_profit_crossings}  first_ts={s.first_take_profit_ts}  current_mc_max={s.current_mc_sol_max}")
        else:
            print("\n-- take profit threshold crossings (current_mc_sol >= threshold) --")
            print("none")

    # Compact “shareable” summary line per mint if single mint requested
    if args.mint and mints:
        s = mints[0]
        print("\n-- mint summary --")
        print(json.dumps(asdict(s), ensure_ascii=False, indent=2))

    if args.out_json:
        out = {s.mint: asdict(s) for s in mints}
        with open(args.out_json, "w", encoding="utf-8") as f:
            json.dump(out, f, ensure_ascii=False, indent=2)
        print(f"\nWrote JSON: {args.out_json}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

