defmodule PlasmDesktop.Mcp.DataPlane do
  @moduledoc """
  Data-plane HTTP client for OSS `plasm-mcp` (`/v1/*`).

  Authenticated tenant MCP upserts (`/internal/mcp-config/v1/*`, API keys) live in
  [`PlasmDesktop.Mcp.ControlPlane`](`PlasmDesktop.Mcp.ControlPlane`) when the agent runs with MCP sqlx.

  URLs resolve per-session via [`PlasmDesktop.Mcp.Config`](`PlasmDesktop.Mcp.Config`).
  """

  @behaviour PlasmUiCore.ToolExplorer

  require Logger

  def mcp_public_base_url(session \\ %{}) when is_map(session) do
    PlasmDesktop.Mcp.Config.resolve_public_base(session)
  end

  def base_url(session \\ %{}) when is_map(session) do
    PlasmDesktop.Mcp.Config.resolve_http_base(session)
  end

  def upstream_mcp_base(session \\ %{}) when is_map(session) do
    PlasmDesktop.Mcp.Config.resolve_mcp_upstream(session)
  end

  def auth_headers(session) when is_map(session) do
    bt = session["plasm_bearer_token"] || session[:plasm_bearer_token]
    ak = session["plasm_api_key"] || session[:plasm_api_key]

    cond do
      is_binary(bt) and bt != "" ->
        [{"authorization", "Bearer " <> bt}]

      is_binary(ak) and ak != "" ->
        [{"x-api-key", ak}]

      true ->
        []
    end
  end

  def format_client_error(%Req.TransportError{reason: :econnrefused}) do
    "Connection refused — no process is accepting TCP on that host/port."
  end

  def format_client_error(%Req.TransportError{reason: :timeout}) do
    "Request timed out before the agent responded."
  end

  def format_client_error(%Req.TransportError{reason: {:tls_alert, alert}}) do
    "TLS error from agent: #{inspect(alert)}."
  end

  def format_client_error(%Req.TransportError{reason: reason}) when is_atom(reason) do
    "HTTP transport failed (#{reason})."
  end

  def format_client_error(%Req.TransportError{reason: reason}) do
    "HTTP transport failed: #{inspect(reason, limit: 200)}."
  end

  def format_client_error(%Mint.TransportError{} = e) do
    "Low-level HTTP transport failed: #{Exception.message(e)}"
  end

  def format_client_error({:http_status, status, body}) do
    "Agent returned HTTP #{status}: #{inspect(body, limit: 400, printable_limit: 800)}"
  end

  def format_client_error(other), do: inspect(other, limit: 400)

  @doc """
  Like `format_client_error/1` but names the resolved discovery base and reminds which port role this URL must be.
  """
  def format_agent_http_error(session, reason) when is_map(session) do
    base = base_url(session)
    summary = format_client_error(reason)

    "#{summary} Using agent HTTP base #{base} (GET #{base}/v1/registry). " <>
      "Point this at the agent `--http` discovery port, not Streamable MCP (`--mcp`) or this desktop UI. " <>
      "Adjust under Settings or PLASM_MCP_HTTP_BASE_URL."
  end

  @impl PlasmUiCore.ToolExplorer
  def list_registry(session) when is_map(session) do
    path = "/v1/registry"
    url = base_url(session) <> path

    case Req.get(url, headers: auth_headers(session), receive_timeout: 60_000) do
      {:ok, %{status: 200, body: body}} when is_map(body) ->
        {:ok, body}

      {:ok, %{status: status, body: body}} ->
        {:error, {:http_status, status, body}}

      {:error, reason} = err ->
        Logger.warning("[plasm_desktop] list_registry failed #{inspect(reason)}")
        err
    end
  end

  @impl PlasmUiCore.ToolExplorer
  def fetch_tool_model(session, entry_id, opts)
      when is_map(session) and is_binary(entry_id) and is_list(opts) do
    # OSS agent accepts focus=all|single|seeds only (`tool_model.rs`); there is no auth-only slice.
    focus = Keyword.get(opts, :focus, "all")
    entities = Keyword.get(opts, :entity, [])

    qs =
      ["focus=" <> URI.encode(focus)] ++
        Enum.map(entities, &("entity=" <> URI.encode(to_string(&1))))

    path =
      "/v1/registry/" <> URI.encode(entry_id) <> "/tool-model?" <> Enum.join(qs, "&")

    url = base_url(session) <> path

    case Req.get(url, headers: auth_headers(session), receive_timeout: 120_000) do
      {:ok, %{status: 200, body: body}} when is_map(body) ->
        {:ok, body}

      {:ok, %{status: status, body: body}} ->
        {:error, {:http_status, status, body}}

      {:error, reason} = err ->
        Logger.warning("[plasm_desktop] fetch_tool_model failed #{inspect(reason)}")
        err
    end
  end

  def fetch_tool_model(session, entry_id) when is_map(session) and is_binary(entry_id) do
    fetch_tool_model(session, entry_id, [])
  end

  @doc """
  `GET /v1/traces` — session-authenticated trace summaries (same surface as SaaS Your MCP).
  """
  def list_traces(session, opts \\ []) when is_map(session) and is_list(opts) do
    offset = Keyword.get(opts, :offset, 0)
    limit = Keyword.get(opts, :limit, 50)

    qs =
      URI.encode_query(%{
        "offset" => offset,
        "limit" => limit
      })

    path = "/v1/traces?" <> qs
    url = base_url(session) <> path

    case Req.get(url, headers: auth_headers(session), receive_timeout: 30_000) do
      {:ok, %{status: 200, body: body}} when is_map(body) ->
        {:ok, body}

      {:ok, %{status: status, body: body}} ->
        {:error, {:http_status, status, body}}

      {:error, reason} = err ->
        Logger.warning("[plasm_desktop] list_traces failed #{inspect(reason)}")
        err
    end
  end

  @doc "`GET /v1/traces/:trace_id` — full trace payload."
  def fetch_trace_detail(session, trace_id) when is_map(session) and is_binary(trace_id) do
    path = "/v1/traces/" <> URI.encode(trace_id)
    url = base_url(session) <> path

    case Req.get(url, headers: auth_headers(session), receive_timeout: 60_000) do
      {:ok, %{status: 200, body: body}} when is_map(body) ->
        {:ok, body}

      {:ok, %{status: 404}} ->
        {:error, :not_found}

      {:ok, %{status: status, body: body}} ->
        {:error, {:http_status, status, body}}

      {:error, reason} = err ->
        Logger.warning("[plasm_desktop] fetch_trace_detail failed #{inspect(reason)}")
        err
    end
  end
end
