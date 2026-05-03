defmodule PlasmDesktopWeb.PageController do
  use PlasmDesktopWeb, :controller

  def home(conn, _params) do
    render(conn, :home)
  end
end
