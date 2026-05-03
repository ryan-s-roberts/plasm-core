defmodule PlasmDesktop.Appliance.PolicySnapshot do
  @moduledoc """
  Loads registry + control-plane MCP policy state for the single appliance config.
  """

  alias PlasmDesktop.Appliance.McpPayload
  alias PlasmDesktop.Mcp.ControlPlane
  alias PlasmDesktop.Mcp.DataPlane
  alias PlasmDesktop.Settings

  defstruct [
    :registry_entries,
    :registry_load_state,
    :registry_error,
    :control_plane_ok,
    :config_id,
    :endpoint_hash_hex,
    :detail,
    :selected_ids,
    :policy_name,
    :policy_status,
    :agent_detail_note
  ]

  @spec from_session(map()) :: %__MODULE__{}
  def from_session(session) when is_map(session) do
    db = Settings.get_all_map()
    cp_ok = match?({:ok, _}, ControlPlane.control_plane_headers())

    {reg_st, entries, reg_err} =
      case DataPlane.list_registry(session) do
        {:ok, %{"entries" => e}} when is_list(e) -> {:ok, e, nil}
        {:ok, other} -> {:error, [], "unexpected registry: #{inspect(other, limit: 200)}"}
        {:error, r} -> {:error, [], DataPlane.format_agent_http_error(session, r)}
      end

    base = %__MODULE__{
      registry_entries: entries,
      registry_load_state: reg_st,
      registry_error: reg_err,
      control_plane_ok: cp_ok,
      config_id: nil,
      endpoint_hash_hex: "",
      detail: nil,
      selected_ids: MapSet.new(),
      policy_name: "Appliance MCP",
      policy_status: "active",
      agent_detail_note: nil
    }

    cond do
      not cp_ok ->
        %{
          base
          | agent_detail_note:
              "Control plane secret is missing or shorter than 16 characters — check Phoenix logs and `desktop_settings`."
        }

      true ->
        case McpPayload.resolve_config_id(db) do
          {:ok, id} ->
            if McpPayload.valid_uuid?(id) do
              case ControlPlane.fetch_config_detail(session, id) do
                {:ok, d} ->
                  sel =
                    d
                    |> McpPayload.allowed_entry_ids_from_detail()
                    |> MapSet.new()

                  eh = Map.get(d, "endpoint_secret_hash_hex") || ""

                  %{
                    base
                    | config_id: id,
                      endpoint_hash_hex: eh,
                      detail: d,
                      selected_ids: sel,
                      policy_name: Map.get(d, "name") || base.policy_name,
                      policy_status: Map.get(d, "status") || "active",
                      agent_detail_note: nil
                  }

                {:error, :not_found} ->
                  eh =
                    case McpPayload.resolve_endpoint_hash_hex(db) do
                      {:ok, h} -> h
                      _ -> ""
                    end

                  %{
                    base
                    | config_id: id,
                      endpoint_hash_hex: eh,
                      agent_detail_note:
                        "Config id is saved locally but the agent has no row yet — connect an app or provision keys from Connect APIs to create it."
                  }

                {:error, reason} ->
                  %{
                    base
                    | config_id: id,
                      agent_detail_note: "Could not load policy: #{inspect(reason, limit: 120)}"
                  }
              end
            else
              %{
                base
                | agent_detail_note:
                    "Stored appliance config id is not a valid UUID; fix or clear it in the database."
              }
            end

          :missing ->
            eh =
              case McpPayload.resolve_endpoint_hash_hex(db) do
                {:ok, h} -> h
                _ -> ""
              end

            %{base | endpoint_hash_hex: eh}
        end
    end
  end
end
