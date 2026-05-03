defmodule PlasmDesktop.Appliance.ControlPlaneSecretLoader do
  @moduledoc false
  use GenServer

  alias PlasmDesktop.Appliance.ControlPlaneSecret

  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @impl true
  def init(_) do
    :ok = ControlPlaneSecret.bootstrap!()
    {:ok, %{}}
  end
end
