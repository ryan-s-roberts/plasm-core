# plasm-runtime

Execution engine for Plasm: HTTP/CML dispatch, **graph cache**, record/replay, and response normalization.

## Design boundary: no domain leakage

Plasm is a **general-purpose language and runtime for API mapping** (schema, expressions, CML, execution). **Domain-specific knowledge is forbidden in this crate:** no branches on particular CGS entity or capability names from `apis/…`, no field-alias or env-key hacks for one vendor’s HTTP templates, and no special transport cases tied to a single product.

Catalog behavior belongs in **`apis/<name>/`**, fixtures, and optional **plugins**—expressed as data and schema-driven rules. Code here stays **agnostic**, driven only by loaded CGS and generic IR/types.

## Graph cache invariants (semi-formal)

Authoritative **module-level** documentation with the same statements (plus implementation notes) lives in [`src/cache.rs`](src/cache.rs) at the top of the `cache` module. Below is a stable summary for readers browsing the repo.

### Structural

| ID | Statement |
|----|-----------|
| **I1** | For every `(k, v)` in the cache’s entity map, `v.reference == k`. |
| **I2** | If a `Ref` exists in the entity map, the type index lists that ref under `entity_type`; removals clear both maps consistently. The type list may contain duplicate refs after repeated merges—resolve via the entity map. |

### Temporal / versioning

| ID | Statement |
|----|-----------|
| **I3** | The internal timestamp source used on insert paths is strictly monotonic per cache instance. |
| **I4** | Per-row `version` / `last_updated` follow [`CachedEntity::merge`] rules. |

### Concurrency (callers)

| ID | Statement |
|----|-----------|
| **I5** | `GraphCache` is **not** thread-safe: at most one exclusive `&mut` access at a time unless the caller provides external locking or sharding. |
| **I6** | Cloned caches are independent; merging forks back into a session cache requires an explicit ordering policy. |

### Merge

| ID | Statement |
|----|-----------|
| **I7** | Insert either adds a row or merges into an existing row; unspecified relations are omitted from the stored relation map. |

## Crate docs

Run `cargo doc -p plasm-runtime --open` for full API documentation; the crate root [`lib.rs`](src/lib.rs) describes the execute pipeline and points at [`GraphCache`].
