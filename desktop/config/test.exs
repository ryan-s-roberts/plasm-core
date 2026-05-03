import Config

config :plasm_desktop, PlasmDesktopWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: 4003],
  secret_key_base: "plasm_desktop_test_secret_key_base_must_be_64_chars_minimum__________",
  server: false

config :plasm_desktop, PlasmDesktop.Repo,
  username: "postgres",
  password: "postgres",
  hostname: "localhost",
  database: "plasm_desktop_test#{System.get_env("MIX_TEST_PARTITION")}",
  pool: Ecto.Adapters.SQL.Sandbox,
  pool_size: System.schedulers_online() * 2

config :logger, level: :warning

config :plasm_desktop, :appliance_control_plane_dev_fallback, true
