## Snapshot JSONL analyzer (for sharing + debugging)

Your bot writes snapshots as **JSONL** (one JSON object per line), usually named:
- `snapshots_YYYY-MM-DD.jsonl`

This tool turns a huge file into a small **text report** you can paste to chat.

### Run (Windows PowerShell)

From `Fullsnajperista/`:

```powershell
python .\tools\analyze_snapshots.py .\snapshots_2026-01-17.jsonl
```

### Analyze one mint only (recommended)

```powershell
python .\tools\analyze_snapshots.py .\snapshots_2026-01-17.jsonl --mint DRdCcU9K3F2kZJVQiEzRxFyL5ENQptBMytcygVVVpump --top 20
```

### Save full per-mint stats to JSON (so you can send just the report file)

```powershell
python .\tools\analyze_snapshots.py .\snapshots_2026-01-17.jsonl --out-json .\snapshot_report.json
```

### What to look for

- `peak_mc_sol decreases detected`: should be **0 violations** after the peak tracking fix.
- `top winners/losers`: quick sanity check that PnL looks reasonable.

