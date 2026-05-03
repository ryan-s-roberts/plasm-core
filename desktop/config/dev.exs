import Config

config :plasm_desktop, PlasmDesktopWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: String.to_integer(System.get_env("PORT") || "4000")],
  check_origin: false,
  code_reloader: true,
  debug_errors: true,
  secret_key_base: "plasm_desktop_dev_secret_key_base_must_be_64_chars_long_minimum______ok",
  watchers: [
    esbuild: {Esbuild, :install_and_run, [:plasm_desktop, ~w(--sourcemap=inline --watch)]},
    tailwind: {Tailwind, :install_and_run, [:plasm_desktop, ~w(--watch)]}
  ]

if database_url = System.get_env("DATABASE_URL") do
  config :plasm_desktop, PlasmDesktop.Repo,
    url: database_url,
    stacktrace: true,
    show_sensitive_data_on_connection_error: true,
    pool_size: 10
else
  config :plasm_desktop, PlasmDesktop.Repo,
    username: "postgres",
    password: "postgres",
    hostname: "localhost",
    database: "plasm_desktop_dev",
    stacktrace: true,
    show_sensitive_data_on_connection_error: true,
    pool_size: 10
end

config :plasm_desktop, dev_routes: true

config :plasm_desktop, :appliance_control_plane_dev_fallback, true
