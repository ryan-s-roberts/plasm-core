defmodule PlasmDesktop.Application do
  @moduledoc false

  use Application

  @impl true
  def start(_type, _args) do
    children = [
      PlasmDesktop.Repo,
      PlasmDesktop.Appliance.ControlPlaneSecretLoader,
      {Phoenix.PubSub, name: PlasmDesktop.PubSub},
      PlasmDesktopWeb.Endpoint
    ]

    opts = [strategy: :one_for_one, name: PlasmDesktop.Supervisor]
    Supervisor.start_link(children, opts)
  end

  @impl true
  def config_change(changed, _new, removed) do
    PlasmDesktopWeb.Endpoint.config_change(changed, removed)
    :ok
  end
end
