defmodule PlasmDesktop.Appliance.McpPayload do
  @moduledoc """
  Builds MCP config upsert bodies for the appliance synthetic tenant triple.

  Shape matches SaaS [`ProjectMcp.payload_for_agent/1`](`PlasmWeb.ProjectMcp.payload_for_agent/1`).
  """

  @spec resolve_config_id(map()) :: {:ok, String.t()} | :missing
  def resolve_config_id(db) when is_map(db) do
    env = nonempty(System.get_env("PLASM_APPLIANCE_MCP_CONFIG_ID"))
    stored = nonempty(Map.get(db, "mcp_appliance_config_id"))

    cond do
      is_binary(env) -> {:ok, env}
      is_binary(stored) -> {:ok, stored}
      true -> :missing
    end
  end

  @spec resolve_endpoint_hash_hex(map()) :: {:ok, String.t()}
  def resolve_endpoint_hash_hex(db) when is_map(db) do
    env = nonempty(System.get_env("PLASM_APPLIANCE_MCP_ENDPOINT_HASH_HEX"))
    stored = nonempty(Map.get(db, "mcp_appliance_endpoint_hash_hex"))
    {:ok, env || stored || ""}
  end

  @spec valid_uuid?(term()) :: boolean()
  def valid_uuid?(s) when is_binary(s) do
    match?({:ok, _}, Ecto.UUID.cast(String.trim(s)))
  end

  def valid_uuid?(_), do: false

  @spec valid_endpoint_hash_hex?(String.t()) :: boolean()
  def valid_endpoint_hash_hex?(s) when is_binary(s) do
    t = String.trim(s)
    String.match?(t, ~r/^[0-9a-fA-F]{64}$/)
  end

  def valid_endpoint_hash_hex?(_), do: false

  @spec generate_endpoint_hash_hex() :: String.t()
  def generate_endpoint_hash_hex do
    :crypto.strong_rand_bytes(32) |> Base.encode16(case: :lower)
  end

  @spec allowed_entry_ids_from_detail(map()) :: [String.t()]
  def allowed_entry_ids_from_detail(d) when is_map(d) do
    (d["allowed_graphs"] || [])
    |> Enum.filter(fn g -> Map.get(g, "enabled", true) end)
    |> Enum.map(&to_string(&1["entry_id"] || ""))
    |> Enum.reject(&(&1 == ""))
    |> Enum.uniq()
  end

  @spec config_version_from_detail(map()) :: non_neg_integer()
  def config_version_from_detail(d) when is_map(d) do
    case d["version"] do
      v when is_integer(v) and v >= 0 ->
        v

      v when is_binary(v) ->
        case Integer.parse(String.trim(v)) do
          {i, _} when i >= 0 -> i
          _ -> 0
        end

      _ ->
        0
    end
  end

  @spec upsert_map(
          non_neg_integer(),
          String.t(),
          String.t(),
          [String.t()],
          keyword()
        ) :: map()
  def upsert_map(version, config_id, endpoint_hex, allowed_entry_ids, opts \\ [])
      when is_integer(version) and is_binary(config_id) and is_binary(endpoint_hex) and
             is_list(allowed_entry_ids) do
    name = Keyword.get(opts, :name, "Appliance MCP")
    status = Keyword.get(opts, :status, "active")

    auth_config_by_entry =
      case Keyword.get(opts, :auth_config_by_entry) do
        %{} = m -> m
        _ -> %{}
      end

    auth_optional =
      case Keyword.get(opts, :auth_optional_entry_ids) do
        list when is_list(list) -> Enum.map(list, &to_string/1)
        _ -> []
      end

    caps =
      case Keyword.get(opts, :capabilities_by_entry) do
        %{} = m -> m
        _ -> %{}
      end

    tenant = Application.fetch_env!(:plasm_desktop, :appliance_tenant_id)
    ws = Application.fetch_env!(:plasm_desktop, :appliance_workspace_slug)
    ps = Application.fetch_env!(:plasm_desktop, :appliance_project_slug)

    %{
      "id" => config_id,
      "tenant_id" => tenant,
      "space_type" => "organization",
      "owner_subject" => nil,
      "version" => version,
      "endpoint_secret_hash_hex" => String.downcase(String.trim(endpoint_hex)),
      "credential_secret_hashes_hex" => [],
      "allowed_entry_ids" => allowed_entry_ids,
      "capabilities_by_entry" => caps,
      "auth_config_by_entry" => auth_config_by_entry,
      "workspace_slug" => ws,
      "project_slug" => ps,
      "name" => name,
      "status" => status,
      "auth_optional_entry_ids" => auth_optional
    }
  end

  defp nonempty(v) when is_binary(v) do
    t = String.trim(v)
    if t == "", do: nil, else: t
  end

  defp nonempty(_), do: nil
end
