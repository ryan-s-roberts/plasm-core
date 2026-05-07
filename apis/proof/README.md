# Proof catalog (Plasm)

This package targets the **public HTTP surface** described in [EveryInc/proof-sdk](https://github.com/EveryInc/proof-sdk) (`AGENT_CONTRACT.md`, `docs/agent-docs.md`): `POST /documents`, `GET /documents/:slug/state`, `GET /documents/:slug/snapshot`, `POST /documents/:slug/edit` and `…/edit/v2`, `POST /documents/:slug/ops`, bridge routes under `…/bridge/*`, and collaboration polling `…/events/pending` / `…/events/ack`.

`mappings.yaml` is generated from `_gen_mappings.py` so paths stay consistent:

```bash
python3 plasm-oss/apis/proof/_gen_mappings.py
```

## Local Proof SDK

From a clone of proof-sdk:

```bash
npm install
npm run serve   # API default http://127.0.0.1:4000 (see proof-sdk README)
```

Point Plasm at that origin (overrides `domain.yaml` `http_backend` for the session):

```bash
export PROOF_API_TOKEN=   # if PROOF_SHARE_MARKDOWN_AUTH_MODE=api_key
cargo run -p plasm-agent --bin plasm-mcp -- --schema apis/proof --http --port 3000 --backend http://127.0.0.1:4000
```

Validate the catalog only:

```bash
cargo run -p plasm-cli --bin plasm -- schema validate apis/proof
```

## curl smoke tests

Create a document (JSON):

```bash
curl -sS -X POST http://127.0.0.1:4000/documents \
  -H 'Content-Type: application/json' \
  -d '{"markdown":"# Hello\n\nfrom curl.","title":"smoke"}' | jq .
```

Share preview `GET /d/:slug` (`document_get` and `document_get_markdown` both use **`Accept: application/json`** in Plasm mappings so responses decode as JSON rows; query `token` when using link access):

```bash
SLUG=…
TOKEN=…   # optional; maps to Plasm `share_token`
curl -sS -H 'Accept: application/json' "http://127.0.0.1:4000/d/$SLUG?token=$TOKEN" | jq .
```

Optional SDK-only raw body (not used by the Plasm catalog):

```bash
curl -sS -H 'Accept: text/markdown' "http://127.0.0.1:4000/d/$SLUG?token=$TOKEN"
```

Agent state + snapshot (canonical `documents` tree):

```bash
curl -sS -H "Authorization: Bearer $PROOF_API_TOKEN" -H 'Accept: application/json' \
  "http://127.0.0.1:4000/documents/$SLUG/state" | jq .
curl -sS -H "Authorization: Bearer $PROOF_API_TOKEN" -H 'Accept: application/json' \
  "http://127.0.0.1:4000/documents/$SLUG/snapshot" | jq .
```

## Domain ↔ SDK notes

- **Optimistic locking:** block-level mutations use Proof SDK **`baseRevision`** (integer) from `GET …/snapshot` — typed in DOMAIN via shared `values.nv_proof_int` on `EditorState.revision`. Structured `POST …/edit` paths that surface **`baseUpdatedAt`** should use the same short-string primitive (`values.nv_proof_str`) when that parameter is modeled on the capability.
- **Share token:** optional capability parameter `share_token` is wired as query **`token`** on requests that support link-style access.
- **Agent identity:** `agent_id` is sent as **`X-Agent-Id`** on mutating routes.
- **Idempotency:** explicit capability parameter `idempotency_key` is sent as **`Idempotency-Key`** when set. On HTTP/MCP execute, Plasm also injects CML env keys `plasm_execute_prompt_hash` and `plasm_execute_session_id`; the generated **`document_edit_*`** mappings derive a default `Idempotency-Key` from those plus mutation fields (`baseRevision`, refs, text, …) when the caller omits `idempotency_key`. Align with Proof rollout: read `contract.idempotencyRequired` and `contract.mutationStage` from `GET …/state` — during required stages the wire must still carry a key (host-derived or explicit). Same key with a different payload hash yields `IDEMPOTENCY_KEY_REUSED`.
- **`document_edit_find_replace_in_doc`:** CML currently emits a single structured `replace` op; optional sweep fields in the domain are not yet mapped — extend `_gen_mappings.py` when you confirm the live JSON shape.
- **`annotation_comment_unresolve`** / **`annotation_comment_batch_apply`** / **bug report** routes are **best-effort** placeholders (`/ops` payload types or `/report/bug` paths) — verify against your Proof revision and adjust `_gen_mappings.py` if the server returns 4xx.

## Hosted Proof

The default `http_backend` in `domain.yaml` remains `https://www.proofeditor.ai`. Hosted deployments may expose compatibility aliases (`/api/agent/*`, `/share/markdown`); this catalog follows the **canonical SDK paths** above.
