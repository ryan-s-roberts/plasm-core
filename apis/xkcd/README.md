# xkcd — Plasm CGS Schema

A [Plasm](../../README.md) domain model for the [xkcd JSON API](https://xkcd.com/json.html).

```bash
cargo run --bin plasm-agent -- \
  --schema apis/xkcd \
  --backend https://xkcd.com \
  --repl
```

Public read-only; no auth.
