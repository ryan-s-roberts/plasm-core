# Open-Meteo — Plasm CGS Schema

A [Plasm](../../README.md) domain model for [Open-Meteo](https://open-meteo.com/) (weather and geocoding APIs).

```bash
cargo run --bin plasm-repl -- \
  --schema apis/openmeteo \
  --backend https://api.open-meteo.com
```

Example one-shot forecast (shell: use `=-74` so longitude is not parsed as a flag):

```bash
plasm-cgs --schema apis/openmeteo --backend https://api.open-meteo.com \
  forecast query --latitude 40.7 --longitude=-74 --current_weather
```

No API key for non-commercial use; see Open-Meteo terms for production. See [apis/README.md](../README.md) for catalog layout and [docs/saas-architecture.md](../../docs/saas-architecture.md) for hosted deployment context.
