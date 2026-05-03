defmodule PlasmDesktop.Mcp.ControlPlane do
  @moduledoc """
  Agent **`/internal/*`** control-plane HTTP (MCP config + API keys + OAuth link start).

  Uses `PLASM_MCP_CONTROL_PLANE_SECRET` / `:mcp_control_plane_secret` like SaaS
  [`PlasmWeb.PlasmMcpClient`](`PlasmWeb.PlasmMcpClient`).
  """

  require Logger

  alias PlasmDesktop.Mcp.Config

  @spec control_plane_headers() :: {:ok, [{String.t(), String.t()}]} | {:error, :missing_secret}
  def control_plane_headers do
    secret =
      case trimmed_env_secret() do
        s when is_binary(s) and byte_size(s) >= 16 ->
          s

        _ ->
          Application.get_env(:plasm_desktop, :mcp_control_plane_secret)
      end

    secret =
      case secret do
        s when is_binary(s) -> String.trim(s)
        _ -> ""
      end

    if secret != "" and String.length(secret) >= 16 do
      {:ok, [{"x-plasm-control-plane-secret", secret}]}
    else
      {:error, :missing_secret}
    end
  end

  defp trimmed_env_secret do
    case System.get_env("PLASM_MCP_CONTROL_PLANE_SECRET") do
      s when is_binary(s) ->
        t = String.trim(s)
        if t != "" and String.length(t) >= 16, do: t, else: nil

      _ ->
        nil
    end
  end

  defp agent_base(session) when is_map(session), do: Config.resolve_http_base(session)

  @spec fetch_config_detail(map(), String.t()) :: {:ok, map()} | {:error, term()}
  def fetch_config_detail(session, id) when is_map(session) do
    with {:ok, hdrs} <- control_plane_headers() do
      path = "/internal/mcp-config/v1/config/" <> URI.encode(to_string(id))
      url = agent_base(session) <> path

      case Req.get(url, headers: hdrs, receive_timeout: 30_000) do
        {:ok, %{status: 200, body: body}} when is_map(body) ->
          {:ok, body}

        {:ok, %{status: 404}} ->
          {:error, :not_found}

        {:ok, %{status: status, body: body}} ->
          {:error, {:http_status, status, body}}

        {:error, reason} = err ->
          Logger.warning("[plasm_desktop] fetch_config_detail #{inspect(reason)}")
          err
      end
    end
  end

  @spec upsert_config(map(), map()) :: {:ok, term()} | {:error, term()}
  def upsert_config(session, body) when is_map(session) and is_map(body) do
    with {:ok, hdrs} <- control_plane_headers() do
      path = "/internal/mcp-config/v1/upsert"
      url = agent_base(session) <> path

      case Req.post(url,
             json: body,
             headers: hdrs,
             receive_timeout: 30_000
           ) do
        {:ok, %{status: code}} when code in [200, 204] ->
          {:ok, :synced}

        {:ok, %{status: status, body: resp}} ->
          {:error, {:http_status, status, resp}}

        {:error, reason} = err ->
          Logger.warning("[plasm_desktop] upsert_config #{inspect(reason)}")
          err
      end
    end
  end

  @spec provision_api_key(map(), map()) :: {:ok, map()} | {:error, term()}
  def provision_api_key(session, %{"config_id" => cid, "label" => lab})
      when is_map(session) and is_binary(lab) do
    t = String.trim(lab)

    if t == "" do
      {:error, :mcp_key_name_required}
    else
      label = String.slice(t, 0, 128)

      with {:ok, hdrs} <- control_plane_headers() do
        path = "/internal/mcp-api-key/v1/provision"
        url = agent_base(session) <> path

        body = %{"config_id" => cid, "label" => label}

        case Req.post(url,
               json: body,
               headers: hdrs,
               receive_timeout: 30_000
             ) do
          {:ok, %{status: 200, body: body}} when is_map(body) ->
            {:ok, body}

          {:ok, %{status: status, body: resp}} ->
            {:error, {:http_status, status, resp}}

          {:error, reason} = err ->
            Logger.warning("[plasm_desktop] provision_api_key #{inspect(reason)}")
            err
        end
      end
    end
  end

  @spec list_api_keys(map(), String.t()) :: {:ok, list(map())} | {:error, term()}
  def list_api_keys(session, config_id) when is_map(session) do
    with {:ok, hdrs} <- control_plane_headers() do
      q = URI.encode_query(%{"config_id" => to_string(config_id)})
      path = "/internal/mcp-api-key/v1/keys?" <> q
      url = agent_base(session) <> path

      case Req.get(url, headers: hdrs, receive_timeout: 30_000) do
        {:ok, %{status: 200, body: body}} when is_list(body) ->
          {:ok, body}

        {:ok, %{status: status, body: resp}} ->
          {:error, {:http_status, status, resp}}

        {:error, reason} = err ->
          Logger.warning("[plasm_desktop] list_api_keys #{inspect(reason)}")
          err
      end
    end
  end

  @spec reveal_api_key(map(), String.t(), String.t()) :: {:ok, map()} | {:error, term()}
  def reveal_api_key(session, config_id, key_id)
      when is_map(session) do
    with {:ok, hdrs} <- control_plane_headers() do
      q =
        URI.encode_query(%{
          "config_id" => to_string(config_id),
          "key_id" => to_string(key_id)
        })

      path = "/internal/mcp-api-key/v1/reveal?" <> q
      url = agent_base(session) <> path

      case Req.get(url, headers: hdrs, receive_timeout: 30_000) do
        {:ok, %{status: 200, body: body}} when is_map(body) ->
          {:ok, body}

        {:ok, %{status: status, body: resp}} ->
          {:error, {:http_status, status, resp}}

        {:error, reason} = err ->
          Logger.warning("[plasm_desktop] reveal_api_key #{inspect(reason)}")
          err
      end
    end
  end

  @spec revoke_api_key(map(), String.t(), String.t()) :: :ok | {:error, term()}
  def revoke_api_key(session, config_id, key_id) when is_map(session) do
    with {:ok, hdrs} <- control_plane_headers() do
      path = "/internal/mcp-api-key/v1/keys/revoke"
      url = agent_base(session) <> path

      body = %{"config_id" => config_id, "key_id" => key_id}

      case Req.post(url,
             json: body,
             headers: hdrs,
             receive_timeout: 30_000
           ) do
        {:ok, %{status: code}} when code in [200, 204] ->
          :ok

        {:ok, %{status: status, body: resp}} ->
          {:error, {:http_status, status, resp}}

        {:error, reason} = err ->
          Logger.warning("[plasm_desktop] revoke_api_key #{inspect(reason)}")
          err
      end
    end
  end

  @spec oauth_link_start(map(), String.t(), String.t(), keyword()) ::
          {:ok, map()} | {:error, term()}
  def oauth_link_start(session, entry_id, return_url, opts \\ [])
      when is_map(session) and is_binary(entry_id) and is_binary(return_url) do
    scopes = Keyword.get(opts, :scopes, [])

    body =
      case scopes do
        list when is_list(list) and list != [] ->
          %{"entry_id" => entry_id, "return_url" => return_url, "scopes" => list}

        _ ->
          %{"entry_id" => entry_id, "return_url" => return_url}
      end

    with {:ok, hdrs} <- control_plane_headers() do
      url = agent_base(session) <> "/internal/oauth-link/v1/start"

      case Req.post(url,
             json: body,
             headers: hdrs,
             receive_timeout: 60_000
           ) do
        {:ok, %{status: 200, body: body}} when is_map(body) ->
          {:ok, body}

        {:ok, %{status: status, body: resp}} ->
          {:error, {:http_status, status, resp}}

        {:error, reason} = err ->
          Logger.warning("[plasm_desktop] oauth_link_start #{inspect(reason)}")
          err
      end
    end
  end

  @doc """
  `POST /internal/oauth-link/v1/provider-upsert` — register or disable a runtime OAuth link provider
  for a registry `entry_id` (control-plane auth). Body keys must match the agent contract.
  """
  @spec oauth_link_provider_upsert(map(), map()) :: :ok | {:error, term()}
  def oauth_link_provider_upsert(session, body) when is_map(session) and is_map(body) do
    with {:ok, hdrs} <- control_plane_headers() do
      url = agent_base(session) <> "/internal/oauth-link/v1/provider-upsert"

      case Req.post(url,
             json: body,
             headers: hdrs,
             receive_timeout: 30_000
           ) do
        {:ok, %{status: code}} when code in [200, 204] ->
          :ok

        {:ok, %{status: status, body: resp}} ->
          {:error, {:http_status, status, resp}}

        {:error, reason} = err ->
          Logger.warning("[plasm_desktop] oauth_link_provider_upsert #{inspect(reason)}")
          err
      end
    end
  end

  @spec outbound_secret_put(map(), String.t(), String.t()) :: :ok | {:error, term()}
  def outbound_secret_put(session, key, value)
      when is_map(session) and is_binary(key) and is_binary(value) do
    with {:ok, hdrs} <- control_plane_headers() do
      path = "/internal/outbound-secrets/v1/put"
      url = agent_base(session) <> path

      case Req.post(url,
             json: %{"key" => key, "value" => value},
             headers: hdrs,
             receive_timeout: 30_000
           ) do
        {:ok, %{status: code}} when code in [200, 204] ->
          :ok

        {:ok, %{status: status, body: resp}} ->
          {:error, {:http_status, status, resp}}

        {:error, reason} = err ->
          Logger.warning("[plasm_desktop] outbound_secret_put #{inspect(reason)}")
          err
      end
    end
  end

  @spec outbound_secret_delete(map(), String.t()) :: :ok | {:error, term()}
  def outbound_secret_delete(session, key) when is_map(session) and is_binary(key) do
    with {:ok, hdrs} <- control_plane_headers() do
      path = "/internal/outbound-secrets/v1/delete"
      url = agent_base(session) <> path

      case Req.post(url,
             json: %{"key" => key},
             headers: hdrs,
             receive_timeout: 30_000
           ) do
        {:ok, %{status: code}} when code in [200, 204] ->
          :ok

        {:ok, %{status: status, body: resp}} ->
          {:error, {:http_status, status, resp}}

        {:error, reason} = err ->
          Logger.warning("[plasm_desktop] outbound_secret_delete #{inspect(reason)}")
          err
      end
    end
  end
end
