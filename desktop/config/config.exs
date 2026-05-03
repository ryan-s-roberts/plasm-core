import Config

config :plasm_desktop,
  ecto_repos: [PlasmDesktop.Repo],
  generators: [timestamp_type: :utc_datetime]

config :plasm_desktop, :appliance_tenant_id, "appliance-local"
config :plasm_desktop, :appliance_workspace_slug, "default"
config :plasm_desktop, :appliance_project_slug, "default"

config :plasm_desktop, :public_desktop_base_url, "http://127.0.0.1:4000"

# When true, unset `PLASM_MCP_CONTROL_PLANE_SECRET` uses the same dev default as OSS `plasm-agent`.
config :plasm_desktop, :appliance_control_plane_dev_fallback, false

config :plasm_desktop, PlasmDesktopWeb.Gettext, default_locale: "en"

config :plasm_desktop, PlasmDesktopWeb.Endpoint,
  url: [host: "localhost"],
  adapter: Bandit.PhoenixAdapter,
  render_errors: [
    formats: [html: PlasmDesktopWeb.ErrorHTML],
    layout: false
  ],
  pubsub_server: PlasmDesktop.PubSub,
  live_view: [signing_salt: "plasmDeskLvSalt"]

config :logger, :console,
  format: "$time $metadata[$level] $message\n",
  level: :info

# Configure esbuild for the small LiveView client used by the desktop appliance.
config :esbuild,
  version: "0.25.4",
  plasm_desktop: [
    args:
      ~w(js/app.js --bundle --target=es2022 --outdir=../priv/static/assets/js --external:/vendor/*),
    cd: Path.expand("../assets", __DIR__),
    env: %{"NODE_PATH" => [Path.expand("../deps", __DIR__), Mix.Project.build_path()]}
  ]

# Tailwind utilities for SaaS-derived Tool Explorer HEEx in `plasm_ui_core` (desktop has no full web.css).
config :tailwind,
  version: "4.1.12",
  plasm_desktop: [
    args: ~w(
      --input=assets/css/app.css
      --output=priv/static/assets/css/app.css
    ),
    cd: Path.expand("..", __DIR__)
  ]

config :phoenix, :json_library, Jason

import_config "#{config_env()}.exs"
