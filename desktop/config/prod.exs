import Config

config :plasm_desktop, PlasmDesktopWeb.Endpoint,
  server: true,
  check_origin: false,
  code_reloader: false

config :logger, level: :info
