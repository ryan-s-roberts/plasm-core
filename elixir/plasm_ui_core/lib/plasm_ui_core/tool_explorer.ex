defmodule PlasmUiCore.ToolExplorer do
  @moduledoc """
  Behaviour for session-authenticated HTTP clients that speak agent discovery/tool-model (`/v1/registry*`).

  Implemented by SaaS [`PlasmWeb.PlasmMcpDataPlane`](`PlasmWeb.PlasmMcpDataPlane`) and appliance
  [`PlasmDesktop.Mcp.DataPlane`](`PlasmDesktop.Mcp.DataPlane`).
  """

  @type session :: map()

  @callback list_registry(session()) :: {:ok, map()} | {:error, term()}

  @callback fetch_tool_model(session(), entry_id :: String.t(), opts :: keyword()) ::
              {:ok, map()} | {:error, term()}
end
