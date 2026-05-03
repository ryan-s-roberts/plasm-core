# GitHub Pages documentation (`doc-site/`)

Static site for **[plasm-core](https://github.com/ryan-s-roberts/plasm-core)** — published via GitHub Actions to **`gh-pages`**.

## Publish target

- **Repository:** `ryan-s-roberts/plasm-core` (this tree when used as the public OSS repo).
- **Public URL:** `https://ryan-s-roberts.github.io/plasm-core/` (GitHub Pages project site).
- **Versioning:** docs track **`main`**; tag **`docs-vYYYY.MM.dd`** or release tags when you need a frozen snapshot (optional **mike** integration — not enabled by default).

## Doc inclusion policy

Sources under `docs/` are **allowlisted**. Maintainer workflow:

1. Edit canonical markdown under the **private monorepo** `docs/` and `.cursor/skills/plasm-authoring/` as needed.
2. Run **`python scripts/sync_allowlisted_docs.py`** from `doc-site/` with monorepo root containing sibling `docs/` (paths adjusted in the script).
3. Commit updates under **`doc-site/docs/`** so the OSS repo stays self-contained for CI.

Excluded from sync (never published): SaaS architecture, OSS/SaaS boundary essays, private control-plane specs, Phoenix/UI UX docs — see project IA.

## Local build

```bash
cd doc-site
python -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
mkdocs serve   # http://127.0.0.1:8000
mkdocs build --strict
```

## Visual design (SaaS-aligned)

[`docs/stylesheets/plasm-saas-theme.css`](docs/stylesheets/plasm-saas-theme.css) mirrors **OKLCH palette tokens** from the Phoenix shell (`web/assets/css/app.css` in the product monorepo): warm primary (light), violet primary (dark), subtle **plasma-style header gradients**, shared radii, and **Plus Jakarta Sans** / **JetBrains Mono**. The Material footer includes a **Plasm Cloud** social link to match hosted UX positioning.

## Editorial constraints

- Examples emphasize **HTTP** and **GraphQL** catalogs; omit experimental transport tutorials from onboarding.
- Appliance onboarding includes OAuth friction (incl. Google Workspace), PAT preference, and **[platform.plasm.tools](https://platform.plasm.tools)** for hosted OAuth management.
