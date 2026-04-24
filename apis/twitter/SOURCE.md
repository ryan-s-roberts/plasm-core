# OpenAPI provenance

- **URL:** `https://api.twitter.com/2/openapi.json`
- **Same source as:** `xdevplatform/twitter-api-typescript-sdk` (`scripts/generate.ts`)
- **Vendored `info.version`:** see `openapi.json` → `info.version` (e.g. **2.161** at last refresh).
- **Servers URL in spec:** `openapi.json` lists `https://api.x.com`; `http_backend` in `domain.yaml` matches that origin.

## Refresh

```bash
curl -fsSL -A 'twitter-api-typescript-sdk/1.2.1' -o apis/twitter/openapi.json \
  'https://api.twitter.com/2/openapi.json'
```

Use a normal User-Agent if the endpoint returns 4xx for bare clients.
