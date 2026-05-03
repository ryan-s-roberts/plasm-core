defmodule PlasmDesktop.Appliance.YourMcpConnect do
  @moduledoc """
  User-intent actions for **Your MCP** on the appliance: connect/revoke catalogs and sync the singleton
  `project_mcp_configs` row via the agent control plane — plus local `project_outbound_*` rows so
  sqlx `hosted_kv_key` resolution matches SaaS.
  """

  import Ecto.Query

  alias PlasmDesktop.Appliance.{
    McpPayload,
    OutboundAuthConfig,
    OutboundConnectedAccount
  }

  alias PlasmDesktop.Mcp.ControlPlane
  alias PlasmDesktop.Repo
  alias PlasmDesktop.Settings

  @spec resolve_storage(map()) ::
          {:ok,
           %{
             config_id: String.t(),
             endpoint_hex: String.t(),
             detail: map() | nil,
             version: non_neg_integer(),
             db: map()
           }}
          | {:error, atom()}
  def resolve_storage(session) when is_map(session) do
    case ControlPlane.control_plane_headers() do
      {:error, _} ->
        {:error, :missing_control_plane_secret}

      {:ok, _} ->
        db = Settings.get_all_map()

        config_id =
          case McpPayload.resolve_config_id(db) do
            {:ok, id} ->
              id = String.trim(to_string(id))
              if McpPayload.valid_uuid?(id), do: id, else: Ecto.UUID.generate()

            :missing ->
              Ecto.UUID.generate()
          end

        endpoint_hex =
          case ControlPlane.fetch_config_detail(session, config_id) do
            {:ok, d} ->
              case Map.get(d, "endpoint_secret_hash_hex") do
                h when is_binary(h) ->
                  t = String.trim(h)
                  if t != "", do: String.downcase(t), else: default_endpoint_hex(db)

                _ ->
                  default_endpoint_hex(db)
              end

            _ ->
              default_endpoint_hex(db)
          end

        endpoint_hex =
          if McpPayload.valid_endpoint_hash_hex?(endpoint_hex) do
            String.downcase(endpoint_hex)
          else
            McpPayload.generate_endpoint_hash_hex()
          end

        detail =
          case ControlPlane.fetch_config_detail(session, config_id) do
            {:ok, d} -> d
            _ -> nil
          end

        version =
          case detail do
            %{} = d -> McpPayload.config_version_from_detail(d) + 1
            _ -> 1
          end

        {:ok,
         %{
           config_id: config_id,
           endpoint_hex: endpoint_hex,
           detail: detail,
           version: version,
           db: db
         }}
    end
  end

  defp default_endpoint_hex(db) do
    case McpPayload.resolve_endpoint_hash_hex(db) do
      {:ok, h} when is_binary(h) ->
        t = String.trim(h)
        if t != "", do: String.downcase(t), else: McpPayload.generate_endpoint_hash_hex()

      _ ->
        McpPayload.generate_endpoint_hash_hex()
    end
  end

  @doc """
  Ensures the singleton appliance MCP config row exists on the agent with **local pointers persisted**.

  If the agent has no row yet, upserts an **empty allowlist** so transport API keys can be provisioned
  immediately — connecting catalogs later only expands `allowed_entry_ids`.
  """
  @spec ensure_config_row(map()) :: :ok | {:error, term()}
  def ensure_config_row(session) when is_map(session) do
    with {:ok, st} <- resolve_storage(session) do
      if st.detail != nil do
        _ = persist_pointers(st.config_id, st.endpoint_hex)
        :ok
      else
        body = upsert_body(st, [], %{}, [], policy_kw(nil))

        case ControlPlane.upsert_config(session, body) do
          {:ok, _} ->
            _ = persist_pointers(st.config_id, st.endpoint_hex)
            :ok

          err ->
            err
        end
      end
    end
  end

  def base_maps(nil), do: {%{}, [], []}

  def base_maps(detail) when is_map(detail) do
    allowed = McpPayload.allowed_entry_ids_from_detail(detail)

    optional =
      detail
      |> Map.get("auth_optional_entry_ids", [])
      |> List.wrap()
      |> Enum.map(&to_string/1)
      |> Enum.reject(&(&1 == ""))

    bindings =
      (detail["auth_bindings"] || [])
      |> Enum.filter(&is_map/1)
      |> Map.new(fn b ->
        {to_string(b["entry_id"] || ""), to_string(b["auth_config_id"] || "")}
      end)
      |> Enum.reject(fn {e, a} -> e == "" or a == "" end)
      |> Map.new()

    {bindings, optional, allowed}
  end

  defp policy_kw(nil), do: [name: "This machine", status: "active"]

  defp policy_kw(detail) when is_map(detail) do
    [
      name: Map.get(detail, "name") || "This machine",
      status: Map.get(detail, "status") || "active"
    ]
  end

  defp persist_pointers(config_id, endpoint_hex) do
    Settings.upsert_many(%{
      "mcp_appliance_config_id" => config_id,
      "mcp_appliance_endpoint_hash_hex" => endpoint_hex
    })
  end

  defp upsert_body(st, allowed, bindings, optional, policy_kw) do
    McpPayload.upsert_map(st.version, st.config_id, st.endpoint_hex, Enum.sort(Enum.uniq(allowed)),
      Keyword.merge(policy_kw,
        auth_config_by_entry: bindings,
        auth_optional_entry_ids: Enum.sort(Enum.uniq(optional))
      )
    )
  end

  @spec connect_public(map(), String.t()) :: :ok | {:error, term()}
  def connect_public(session, entry_id) when is_map(session) do
    eid = normalize_entry_id(entry_id)

    if eid == "" do
      {:error, :bad_entry}
    else
      with {:ok, st} <- resolve_storage(session) do
        {bindings, optional, allowed} = base_maps(st.detail)
        allowed = Enum.uniq([eid | allowed])
        optional = Enum.uniq([eid | optional])
        body = upsert_body(st, allowed, bindings, optional, policy_kw(st.detail))

        case ControlPlane.upsert_config(session, body) do
          {:ok, _} ->
            _ = persist_pointers(st.config_id, st.endpoint_hex)
            :ok

          err ->
            err
        end
      end
    end
  end

  @spec connect_api_key(map(), String.t(), String.t()) :: :ok | {:error, term()}
  def connect_api_key(session, entry_id, secret) when is_map(session) and is_binary(secret) do
    eid = normalize_entry_id(entry_id)
    secret = String.trim(secret)

    cond do
      eid == "" ->
        {:error, :bad_entry}

      secret == "" ->
        {:error, :empty_secret}

      true ->
        auth_id = Ecto.UUID.generate()
        kv_key = "plasm:outbound:v1:" <> auth_id

        with {:ok, st} <- resolve_storage(session),
             :ok <- ControlPlane.outbound_secret_put(session, kv_key, secret),
             {:ok, _} <- insert_auth_and_account(auth_id, eid, "api_key", kv_key, []) do
          {bindings, optional, allowed} = base_maps(st.detail)
          bindings = Map.put(bindings, eid, auth_id)
          optional = Enum.reject(optional, &(&1 == eid))
          allowed = Enum.uniq([eid | allowed])
          body = upsert_body(st, allowed, bindings, optional, policy_kw(st.detail))

          case ControlPlane.upsert_config(session, body) do
            {:ok, _} ->
              _ = persist_pointers(st.config_id, st.endpoint_hex)
              :ok

            err ->
              err
          end
        end
    end
  end

  @spec complete_oauth_return(map(), String.t(), String.t(), list(String.t())) ::
          :ok | {:error, term()}
  def complete_oauth_return(session, entry_id, hosted_kv_key, scopes \\ [])
      when is_map(session) do
    eid = normalize_entry_id(entry_id)
    kv = String.trim(to_string(hosted_kv_key || ""))

    cond do
      eid == "" ->
        {:error, :bad_entry}

      not valid_kv?(kv) ->
        {:error, :bad_kv_key}

      true ->
        auth_id = Ecto.UUID.generate()
        scopes = scopes |> List.wrap() |> Enum.map(&to_string/1)

        with {:ok, st} <- resolve_storage(session),
             {:ok, _} <- insert_auth_and_account(auth_id, eid, "oauth2", kv, scopes) do
          {bindings, optional, allowed} = base_maps(st.detail)
          bindings = Map.put(bindings, eid, auth_id)
          optional = Enum.reject(optional, &(&1 == eid))
          allowed = Enum.uniq([eid | allowed])
          body = upsert_body(st, allowed, bindings, optional, policy_kw(st.detail))

          case ControlPlane.upsert_config(session, body) do
            {:ok, _} ->
              _ = persist_pointers(st.config_id, st.endpoint_hex)
              :ok

            err ->
              err
          end
        end
    end
  end

  @spec revoke(map(), String.t()) :: :ok | {:error, term()}
  def revoke(session, entry_id) when is_map(session) do
    eid = normalize_entry_id(entry_id)

    if eid == "" do
      {:error, :bad_entry}
    else
      with {:ok, st} <- resolve_storage(session) do
        {bindings, optional, allowed} = base_maps(st.detail)
        auth_uuid = Map.get(bindings, eid)

        if is_binary(auth_uuid) and auth_uuid != "" do
          delete_outbound_for_auth(session, auth_uuid)
        end

        bindings = Map.delete(bindings, eid)
        optional = Enum.reject(optional, &(&1 == eid))
        allowed = Enum.reject(allowed, &(&1 == eid))

        body = upsert_body(st, allowed, bindings, optional, policy_kw(st.detail))

        case ControlPlane.upsert_config(session, body) do
          {:ok, _} ->
            _ = persist_pointers(st.config_id, st.endpoint_hex)
            :ok

          err ->
            err
        end
      end
    end
  end

  defp delete_outbound_for_auth(session, auth_uuid) do
    q =
      from(c in OutboundConnectedAccount,
        where: c.auth_config_id == ^auth_uuid and c.status == "active"
      )

    rows = Repo.all(q)

    Enum.each(rows, fn row ->
      k = row.hosted_kv_key

      if is_binary(k) and k != "" do
        _ = ControlPlane.outbound_secret_delete(session, k)
      end
    end)

    _ = Repo.delete_all(from(a in OutboundAuthConfig, where: a.id == ^auth_uuid))
    :ok
  rescue
    e in Postgrex.Error -> {:error, e}
  end

  defp insert_auth_and_account(auth_id, entry_id, auth_kind, kv_key, scopes)
       when is_binary(auth_id) and is_binary(entry_id) do
    {tenant, ws, ps} = appliance_triple()
    now = DateTime.utc_now() |> DateTime.truncate(:microsecond)

    Repo.transaction(fn ->
      Repo.insert!(%OutboundAuthConfig{
        id: auth_id,
        tenant_id: tenant,
        workspace_slug: ws,
        project_slug: ps,
        space_type: "organization",
        owner_subject: nil,
        registry_entry_id: entry_id,
        auth_kind: auth_kind,
        name: "#{entry_id} · Appliance",
        status: "enabled",
        oauth_scope_set_name: nil,
        oauth_scopes: scopes,
        inserted_at: now,
        updated_at: now
      })

      Repo.insert!(%OutboundConnectedAccount{
        id: Ecto.UUID.generate(),
        auth_config_id: auth_id,
        owner_subject: nil,
        external_user_id: nil,
        hosted_kv_key: kv_key,
        status: "active",
        granted_scopes: scopes,
        last_connected_at: now,
        inserted_at: now,
        updated_at: now
      })
    end)
  end

  defp appliance_triple do
    {Application.fetch_env!(:plasm_desktop, :appliance_tenant_id),
     Application.fetch_env!(:plasm_desktop, :appliance_workspace_slug),
     Application.fetch_env!(:plasm_desktop, :appliance_project_slug)}
  end

  defp normalize_entry_id(id) do
    id |> to_string() |> String.trim()
  end

  defp valid_kv?(k) when is_binary(k) do
    String.starts_with?(k, "plasm:outbound:") or String.starts_with?(k, "plasm:oauth_app:v1:")
  end

  defp valid_kv?(_), do: false
end
