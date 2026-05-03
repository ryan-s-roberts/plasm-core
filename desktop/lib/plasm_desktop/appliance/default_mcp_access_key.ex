defmodule PlasmDesktop.Appliance.DefaultMcpAccessKey do
  @moduledoc """
  Bootstraps the singleton appliance MCP row on the agent (empty catalog allowlist), then
  provisions a first **`desktop`** transport key when none exist — **before** the user connects APIs.
  """

  alias PlasmDesktop.Appliance.{McpPayload, YourMcpConnect, YourMcpSnapshot}
  alias PlasmDesktop.Mcp.ControlPlane

  @default_label "desktop"

  @doc """
  Upserts baseline MCP config when missing, reloads snapshot, provisions `desktop` when the key list is empty.

  Returns `{:ok, api_key, snap}` (show key once), `{:noop, snap}`, or `{:error, reason, snap}`.
  """
  @spec bootstrap(map()) ::
          {:ok, String.t(), struct()}
          | {:noop, struct()}
          | {:error, term(), struct()}
  def bootstrap(session) when is_map(session) do
    ensure_result = YourMcpConnect.ensure_config_row(session)

    if ensure_result != :ok do
      require Logger

      Logger.warning(
        "[plasm_desktop] appliance MCP config bootstrap: #{inspect(ensure_result, limit: 120)}"
      )
    end

    snap = YourMcpSnapshot.build(session)

    case maybe_provision_from_snap(session, snap) do
      {:ok, key} ->
        {:ok, key, YourMcpSnapshot.build(session)}

      :noop ->
        {:noop, snap}

      {:error, reason} ->
        {:error, reason, snap}
    end
  end

  defp maybe_provision_from_snap(session, snap) do
    policy = snap.policy

    cond do
      not policy.control_plane_ok ->
        :noop

      not is_binary(policy.config_id) or not McpPayload.valid_uuid?(policy.config_id) ->
        :noop

      policy.detail == nil ->
        :noop

      snap.api_keys_error != nil ->
        :noop

      snap.api_keys != [] ->
        :noop

      true ->
        case ControlPlane.provision_api_key(session, %{
               "config_id" => policy.config_id,
               "label" => @default_label
             }) do
          {:ok, body} when is_map(body) ->
            case Map.get(body, "api_key") do
              k when is_binary(k) and k != "" -> {:ok, k}
              _ -> {:error, :missing_api_key_in_response}
            end

          {:error, _} = err ->
            err
        end
    end
  end
end
