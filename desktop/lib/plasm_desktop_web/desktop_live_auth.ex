defmodule PlasmDesktopWeb.DesktopLiveAuth do
  @moduledoc """
  Single-principal desktop shell: no SaaS tenant/workspace context.
  """
  import Phoenix.Component

  def on_mount(:defaults, _params, _session, socket) do
    {:cont,
     socket
     |> assign(:rust_shell_context, %{})
     |> assign(:use_project_shell, false)}
  end
end
