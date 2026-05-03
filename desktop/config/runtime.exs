import Config

if config_env() == :prod do
  secret_key_base =
    System.get_env("SECRET_KEY_BASE") ||
      raise "environment variable SECRET_KEY_BASE is missing"

  host = System.get_env("PHX_HOST") || "example.com"
  port = String.to_integer(System.get_env("PORT") || "4000")

  database_url =
    System.get_env("DATABASE_URL") ||
      raise "environment variable DATABASE_URL is missing at runtime"

  maybe_ipv6 = if System.get_env("ECTO_IPV6") in ~w(true 1), do: [:inet6], else: []

  config :plasm_desktop, PlasmDesktop.Repo,
    url: database_url,
    pool_size: String.to_integer(System.get_env("POOL_SIZE") || "10"),
    socket_options: maybe_ipv6

  config :plasm_desktop, PlasmDesktopWeb.Endpoint,
    url: [host: host, port: 443, scheme: "https"],
    http: [
      ip: {0, 0, 0, 0},
      port: port
    ],
    secret_key_base: secret_key_base
end

config :plasm_desktop, :mcp_control_plane_secret, System.get_env("PLASM_MCP_CONTROL_PLANE_SECRET")

config :plasm_desktop, :public_desktop_base_url,
  System.get_env("PLASM_DESKTOP_PUBLIC_URL") || "http://127.0.0.1:4000"

config :plasm_desktop, :appliance_tenant_id,
  System.get_env("PLASM_APPLIANCE_MCP_TENANT_ID") || "appliance-local"

config :plasm_desktop, :appliance_workspace_slug,
  System.get_env("PLASM_APPLIANCE_MCP_WORKSPACE_SLUG") || "default"

config :plasm_desktop, :appliance_project_slug,
  System.get_env("PLASM_APPLIANCE_MCP_PROJECT_SLUG") || "default"
