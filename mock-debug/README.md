# Mock debug samples

Drop your tracker `.json` file(s) here so we can inspect breakeven / PnL logic.

## What to add
- Copy the JSON file that contains your positions (the one with fields like `buys`, `breakeven_armed`, `pnl_percent`, `sell_reason`).
- If you have multiple, put them in `mock-debug/samples/` and name them clearly, e.g. `case1-breakeven.json`.

## Privacy
If the JSON contains sensitive data (wallets, keys), you can redact them first. We mainly need:
- `enable_breakeven`, `breakeven_arm_mc_usd`, `breakeven_buffer_percent`
- for the affected position(s): `mint`, `timestamp`, `our_buy_sol`, `token_amount`, `token_price_sol`, `mc_at_entry_sol`, `current_price_sol`, `pnl_percent`, `breakeven_armed`, `breakeven_armed_at_mc_sol`, `sell_reason`, `sell_timestamp`


