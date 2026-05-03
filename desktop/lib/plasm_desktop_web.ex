defmodule PlasmDesktopWeb do
  @moduledoc """
  The entrypoint for defining your web interface, such
  as controllers, components, channels, and so on.
  """

  def static_paths, do: ~w(assets robots.txt)

  def router do
    quote do
      use Phoenix.Router, helpers: false

      import Plug.Conn
      import Phoenix.Controller
      import Phoenix.LiveView.Router
    end
  end

  def channel do
    quote do
      use Phoenix.Channel
    end
  end

  def controller do
    quote do
      use Phoenix.Controller,
        formats: [:html, :json],
        layouts: [html: PlasmDesktopWeb.Layouts]

      use Gettext, backend: PlasmDesktopWeb.Gettext

      import Plug.Conn

      unquote(verified_routes())
    end
  end

  def live_view do
    quote do
      use Phoenix.LiveView,
        layout: {PlasmDesktopWeb.Layouts, :app}

      unquote(html_helpers())
    end
  end

  def live_component do
    quote do
      use Phoenix.LiveComponent

      unquote(html_helpers())
    end
  end

  def html do
    quote do
      use Phoenix.Component

      import Phoenix.Controller,
        only: [get_csrf_token: 0, view_module: 1, view_template: 1]

      unquote(html_helpers())
    end
  end

  defp html_helpers do
    quote do
      import Phoenix.HTML
      import PlasmDesktopWeb.CoreComponents
      import PlasmUiCore.Web.CoreComponents, only: [icon: 1]
      import PlasmUiCore.Web.McpRegistryVisuals
      import PlasmUiCore.Web.OauthScopeEditor, only: [oauth_scope_editor: 1]

      alias Phoenix.LiveView.JS
      alias PlasmDesktopWeb.Layouts
    end
  end

  defp verified_routes do
    quote do
      use Phoenix.VerifiedRoutes,
        endpoint: PlasmDesktopWeb.Endpoint,
        router: PlasmDesktopWeb.Router,
        statics: PlasmDesktopWeb.static_paths()
    end
  end

  defmacro __using__(which) when is_atom(which) do
    apply(__MODULE__, which, [])
  end
end
