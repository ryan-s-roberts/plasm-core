defmodule PlasmDesktopWeb.RedirectController do
  @moduledoc false
  use PlasmDesktopWeb, :controller

  @doc "Legacy paths bookmarked during previews — canonical surface is `/connect-apis`."
  def connect_apis(conn, _params), do: redirect(conn, to: "/connect-apis")
end
