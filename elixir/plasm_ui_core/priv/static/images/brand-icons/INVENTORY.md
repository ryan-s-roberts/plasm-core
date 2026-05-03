# Registry brand icons (self-hosted)

SVGs ship with **`plasm_ui_core`** and are served at **`/vendor/plasm-ui-core/images/brand-icons/<file>.svg`** when the host mounts `Plug.Static` with `from: :plasm_ui_core` and `only: ~w(css images)` (see `PlasmUiCore.Assets.static_mount_path/0`).

| File | Registry `entry_id` keys | Source |
|------|---------------------------|--------|
| `github.svg` | `github` | Simple Icons **v11.14.0** (npm `simple-icons`) |
| `gitlab.svg` | `gitlab` | Simple Icons v11.14.0 |
| `linear.svg` | `linear` | Simple Icons v11.14.0 |
| `notion.svg` | `notion` | Simple Icons v11.14.0 |
| `slack.svg` | `slack` | Simple Icons v11.14.0 |
| `gmail.svg` | `gmail` | Simple Icons v11.14.0 |
| `googledocs.svg` | `google-docs` | Simple Icons v11.14.0 |
| `googledrive.svg` | `google-drive` | Simple Icons v11.14.0 |
| `jira.svg` | `jira` | Simple Icons v11.14.0 |
| `clickup.svg` | `clickup` | Simple Icons v11.14.0 |
| `anthropic.svg` | `claude` | Simple Icons v11.14.0 |
| `openai.svg` | `chatgpt`, `codex`, `agent_builder` | Simple Icons v11.14.0 |
| `visualstudiocode.svg` | `vscode` | Simple Icons v11.14.0 |
| `windsurf.svg` | `windsurf` | Simple Icons **v15.0.0** (slug not in v11) |
| `vultr.svg` | `vultr` | Simple Icons v15.0.0 |
| `cursor.svg` | `cursor` | **In-repo** geometric mark (not a third-party logo) |
| `openclaw.svg` | `openclaw` | **In-repo** stylized mark for OpenClaw installer card (not an official logo file) |

**Re-vendor** Simple Icons files (network required):

```bash
./scripts/vendor_brand_icons.sh
```

License: Simple Icons is **[CC0 1.0](https://github.com/simple-icons/simple-icons/blob/develop/LICENSE.md)**. Retain upstream `title` / attribution in SVG where present.
