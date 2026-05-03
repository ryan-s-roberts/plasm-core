defmodule PlasmDesktopWeb.Endpoint do
  use Phoenix.Endpoint, otp_app: :plasm_desktop

  @session_options [
    store: :cookie,
    key: "_plasm_desktop_key",
    signing_salt: "plasmDesktopSess",
    same_site: "Lax"
  ]

  socket "/live", Phoenix.LiveView.Socket,
    websocket: [connect_info: [session: @session_options]],
    longpoll: [connect_info: [session: @session_options]]

  plug Plug.Static,
    at: PlasmUiCore.Assets.static_mount_path(),
    from: :plasm_ui_core,
    gzip: not code_reloading?,
    only: ~w(css images)

  plug Plug.Static,
    at: "/",
    from: :plasm_desktop,
    gzip: not code_reloading?,
    only: PlasmDesktopWeb.static_paths()

  if code_reloading? do
    socket "/phoenix/live_reload/socket", Phoenix.LiveReloader.Socket
    plug Phoenix.LiveReloader
    plug Phoenix.CodeReloader
    plug Phoenix.Ecto.CheckRepoStatus, otp_app: :plasm_desktop
  end

  plug Plug.RequestId
  plug Plug.Telemetry, event_prefix: [:phoenix, :endpoint]

  plug Plug.Parsers,
    parsers: [:urlencoded, :multipart, :json],
    pass: ["*/*"],
    json_decoder: Phoenix.json_library()

  plug Plug.MethodOverride
  plug Plug.Head
  plug Plug.Session, @session_options
  plug PlasmDesktopWeb.Router
end
