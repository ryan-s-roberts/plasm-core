defmodule PlasmDesktopWeb.Router do
  use PlasmDesktopWeb, :router

  pipeline :browser do
    plug(:accepts, ["html"])
    plug(:fetch_session)
    plug(PlasmDesktopWeb.DesktopSessionPlug)
    plug(:fetch_live_flash)
    plug(:put_root_layout, html: {PlasmDesktopWeb.Layouts, :root})
    plug(:protect_from_forgery)
    plug(:put_secure_browser_headers)
  end

  scope "/", PlasmDesktopWeb do
    pipe_through(:browser)

    get("/mcp-policy", RedirectController, :connect_apis)
    get("/mcp-oauth", RedirectController, :connect_apis)

    live_session :appliance do
      live("/", YourMcpLive)
      live("/tools", ToolIndexLive)
      live("/tools/:entry_id", ToolShowLive)
      live("/connect-apis", ConnectApisLive)
      live("/oauth-apps", OauthAppsLive)
      live("/oauth-apps/:entry_id", OauthAppsLive)
      live("/settings", SettingsLive)
      live("/traces", TracesLive)
      live("/traces/:trace_id", TraceShowLive)
    end
  end
end
