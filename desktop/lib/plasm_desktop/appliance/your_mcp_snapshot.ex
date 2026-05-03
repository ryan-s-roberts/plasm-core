defmodule PlasmDesktop.Appliance.YourMcpSnapshot do
  @moduledoc """
  Aggregates discovery, policy, traces, and API key rows for the appliance **Your MCP** home.
  """

  alias PlasmDesktop.Appliance.PolicySnapshot
  alias PlasmDesktop.Mcp.Config
  alias PlasmDesktop.Mcp.ControlPlane
  alias PlasmDesktop.Mcp.DataPlane

  defstruct [
    :http_base,
    :mcp_public_base,
    :registry_entries,
    :registry_ok,
    :registry_error,
    :policy,
    :traces,
    :traces_error,
    :api_keys,
    :api_keys_error
  ]

  @spec build(map()) :: %__MODULE__{}
  def build(session) when is_map(session) do
    policy = PolicySnapshot.from_session(session)

    {reg_ok, entries, reg_err} =
      case DataPlane.list_registry(session) do
        {:ok, %{"entries" => e}} when is_list(e) ->
          {:ok, e, nil}

        {:ok, other} ->
          {:error, [], "unexpected registry: #{inspect(other, limit: 160)}"}

        {:error, r} ->
          {:error, [], DataPlane.format_agent_http_error(session, r)}
      end

    {traces, terr} =
      case DataPlane.list_traces(session, offset: 0, limit: 8) do
        {:ok, %{"traces" => t}} when is_list(t) ->
          {t, nil}

        {:ok, body} when is_map(body) ->
          {Map.get(body, "traces") || [], nil}

        {:error, r} ->
          {[], DataPlane.format_client_error(r)}
      end

    {keys, kerr} =
      cond do
        not policy.control_plane_ok ->
          {[], nil}

        not is_binary(policy.config_id) ->
          {[], nil}

        true ->
          case ControlPlane.list_api_keys(session, policy.config_id) do
            {:ok, list} when is_list(list) -> {list, nil}
            {:error, r} -> {[], DataPlane.format_client_error(r)}
          end
      end

    %__MODULE__{
      http_base: Config.resolve_http_base(session),
      mcp_public_base: Config.resolve_public_base(session),
      registry_entries: entries,
      registry_ok: reg_ok == :ok,
      registry_error: reg_err,
      policy: policy,
      traces: traces,
      traces_error: terr,
      api_keys: keys,
      api_keys_error: kerr
    }
  end
end
