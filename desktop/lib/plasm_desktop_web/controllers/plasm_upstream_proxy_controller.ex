defmodule PlasmDesktopWeb.PlasmUpstreamProxyController do
  @moduledoc """
  OSS desktop reverse-proxy: `/plasm/mcp` → MCP listener; `/plasm/http/oauth/*` → agent HTTP `/oauth/*`.

  OAuth authorize forwards `principal_token` from session when set (no SaaS consent/login routes).
  """
  use PlasmDesktopWeb, :controller

  import Plug.Conn

  require Logger

  plug :put_layout, false

  @mcp_receive_timeout_ms 300_000
  @oauth_receive_timeout_ms 60_000

  @req_hop_by_hop ~w(host connection content-length transfer-encoding te trailer upgrade keep-alive)
  @resp_hop_by_hop ~w(connection transfer-encoding keep-alive proxy-authenticate proxy-authorization trailer upgrade)

  @forward_header_allowlist MapSet.new(~w(
    accept accept-charset accept-encoding accept-language
    authorization content-type user-agent
    traceparent tracestate baggage
    cache-control pragma
  ))

  @mcp_oauth_browser_only_query_params ~w(plasm_ui_return ui_return_to)

  def mcp(conn, _params) do
    case mcp_upstream_path(conn.request_path) do
      {:ok, upstream_path} ->
        {:proxy, conn, extra_query} = maybe_prepare_mcp_authorize(conn, upstream_path)

        proxy_to_upstream(
          conn,
          upstream_mcp_base(conn),
          upstream_path,
          @mcp_receive_timeout_ms,
          extra_query
        )

      :error ->
        conn |> put_resp_content_type("text/plain") |> send_resp(404, "not found") |> halt()
    end
  end

  def oauth(conn, _params) do
    case oauth_upstream_path(conn.request_path) do
      {:ok, upstream_path} ->
        proxy_to_upstream(conn, http_upstream_base(conn), upstream_path, @oauth_receive_timeout_ms)

      :error ->
        conn |> put_resp_content_type("text/plain") |> send_resp(404, "not found") |> halt()
    end
  end

  defp mcp_upstream_path(request_path) do
    cond do
      request_path == "/plasm/mcp" ->
        {:ok, "/mcp"}

      String.starts_with?(request_path, "/plasm/mcp/") ->
        prefix = "/plasm/mcp"
        start = byte_size(prefix)
        len = byte_size(request_path) - start
        suffix = binary_part(request_path, start, len)
        {:ok, "/mcp" <> suffix}

      true ->
        :error
    end
  end

  defp oauth_upstream_path(request_path) do
    prefix = "/plasm/http"

    if String.starts_with?(request_path, prefix) do
      p_len = byte_size(prefix)
      suffix = binary_part(request_path, p_len, byte_size(request_path) - p_len)

      if suffix != "" and String.starts_with?(suffix, "/oauth") do
        {:ok, suffix}
      else
        :error
      end
    else
      :error
    end
  end

  defp proxy_to_upstream(conn, base, path, receive_timeout_ms, extra_query \\ %{}) do
    base = String.trim_trailing(base, "/")
    query = upstream_query(conn, extra_query)
    url = base <> path <> query

    case read_entire_body(conn) do
      {:ok, body, conn} ->
        body = effective_request_body(conn, body)
        method = req_method(conn.method)
        headers = forward_request_headers(conn)

        req_opts =
          [
            method: method,
            url: url,
            headers: headers,
            retry: false,
            redirect: false,
            into: :self,
            receive_timeout: receive_timeout_ms,
            decode_body: false
          ]
          |> maybe_put_body(body)

        case Req.request(req_opts) do
          {:ok, %Req.Response{status: status, headers: resp_headers} = resp} ->
            case resp.body do
              %Req.Response.Async{} = async ->
                conn
                |> merge_streaming_resp_headers(resp_headers)
                |> send_chunked(status)
                |> stream_req_async_to_chunks(resp, async)

              body when is_binary(body) ->
                conn
                |> merge_buffered_resp_headers(resp_headers)
                |> send_resp(status, body)

              _ ->
                conn
                |> merge_buffered_resp_headers(resp_headers)
                |> send_resp(status, "")
            end

          {:error, reason} ->
            level = upstream_proxy_failure_log_level(reason)

            Logger.log(
              level,
              "plasm_desktop upstream proxy failed url=#{redact_sensitive_url(url)} method=#{inspect(method)} reason=#{inspect(reason)}"
            )

            conn
            |> put_resp_content_type("text/plain")
            |> send_resp(502, "upstream error")
            |> halt()
        end

      {:error, :body_too_large} ->
        conn
        |> put_resp_content_type("text/plain")
        |> send_resp(413, "request body too large")
        |> halt()

      {:error, _} ->
        conn
        |> put_resp_content_type("text/plain")
        |> send_resp(400, "bad request body")
        |> halt()
    end
  end

  defp maybe_put_body(opts, ""), do: opts
  defp maybe_put_body(opts, body), do: Keyword.put(opts, :body, body)

  defp effective_request_body(_conn, body) when is_binary(body) and body != "", do: body

  defp effective_request_body(conn, "") do
    content_type =
      conn
      |> get_req_header("content-type")
      |> List.first()
      |> to_string()
      |> String.downcase()

    cond do
      String.contains?(content_type, "application/json") ->
        case conn.body_params do
          %Plug.Conn.Unfetched{} ->
            ""

          params when is_map(params) or is_list(params) ->
            Jason.encode!(params)

          _ ->
            ""
        end

      String.contains?(content_type, "application/x-www-form-urlencoded") ->
        case conn.body_params do
          %Plug.Conn.Unfetched{} -> ""
          params when is_map(params) -> Plug.Conn.Query.encode(params)
          _ -> ""
        end

      true ->
        ""
    end
  end

  defp max_proxy_body_bytes do
    Application.get_env(:plasm_desktop, :plasm_upstream_proxy_max_body_bytes, 8_388_608)
  end

  defp read_entire_body(conn, acc \\ "")

  defp read_entire_body(conn, acc) do
    max = max_proxy_body_bytes()
    read_entire_body(conn, acc, max)
  end

  defp read_entire_body(conn, acc, max) do
    cond do
      byte_size(acc) >= max ->
        {:error, :body_too_large}

      true ->
        case read_body(conn) do
          {:ok, body, conn} ->
            combined = acc <> body

            if byte_size(combined) > max do
              {:error, :body_too_large}
            else
              {:ok, combined, conn}
            end

          {:more, partial, conn} ->
            combined = acc <> partial

            if byte_size(combined) > max do
              {:error, :body_too_large}
            else
              read_entire_body(conn, combined, max)
            end

          {:error, reason} ->
            {:error, reason}
        end
    end
  end

  defp req_method("GET"), do: :get
  defp req_method("POST"), do: :post
  defp req_method("HEAD"), do: :head
  defp req_method("OPTIONS"), do: :options
  defp req_method("PUT"), do: :put
  defp req_method("DELETE"), do: :delete
  defp req_method("PATCH"), do: :patch
  defp req_method(_), do: :get

  defp forward_request_headers(conn) do
    Enum.filter(conn.req_headers, &forward_header?/1)
  end

  defp forward_header?({k, _}) do
    kl = String.downcase(k)

    cond do
      kl in @req_hop_by_hop -> false
      String.starts_with?(kl, "proxy-") -> false
      kl == "cookie" -> false
      MapSet.member?(@forward_header_allowlist, kl) -> true
      String.starts_with?(kl, "x-plasm-") -> true
      String.starts_with?(kl, "mcp-") -> true
      kl == "x-request-id" -> true
      true -> false
    end
  end

  defp merge_buffered_resp_headers(conn, headers) do
    Enum.reduce(headers, conn, fn {k, v}, c ->
      kl = String.downcase(k)
      if kl in @resp_hop_by_hop, do: c, else: safe_put_resp_header(c, kl, v)
    end)
  end

  defp merge_streaming_resp_headers(conn, headers) do
    Enum.reduce(headers, conn, fn {k, v}, c ->
      kl = String.downcase(k)

      if kl in @resp_hop_by_hop or kl == "content-length" do
        c
      else
        safe_put_resp_header(c, kl, v)
      end
    end)
  end

  defp safe_put_resp_header(conn, key, value) when is_binary(value) do
    put_resp_header(conn, key, value)
  rescue
    ArgumentError -> conn
  end

  defp safe_put_resp_header(conn, key, values) when is_list(values) do
    Enum.reduce(values, conn, fn v, c ->
      if is_binary(v), do: safe_put_resp_header(c, key, v), else: c
    end)
  end

  defp safe_put_resp_header(conn, _, _), do: conn

  defp stream_req_async_to_chunks(conn, resp, async) do
    try do
      case Enum.reduce_while(async, {:ok, conn}, fn chunk, {:ok, c} ->
             case Plug.Conn.chunk(c, chunk) do
               {:ok, nc} ->
                 {:cont, {:ok, nc}}

               {:error, _} ->
                 Req.cancel_async_response(resp)
                 {:halt, {:error, c}}
             end
           end) do
        {:ok, final} -> final
        {:error, final} -> final
      end
    rescue
      e ->
        Req.cancel_async_response(resp)
        reraise(e, __STACKTRACE__)
    end
  end

  defp plug_session_map(conn) do
    Map.get(conn.private, :plug_session, %{})
  end

  defp upstream_mcp_base(conn) do
    PlasmDesktop.Mcp.Config.resolve_mcp_upstream(plug_session_map(conn))
  end

  defp http_upstream_base(conn) do
    PlasmDesktop.Mcp.Config.resolve_http_base(plug_session_map(conn))
  end

  defp maybe_prepare_mcp_authorize(conn, "/mcp/oauth/authorize") do
    query_params = URI.decode_query(conn.query_string || "")

    cond do
      present?(query_params["principal_token"]) ->
        {:proxy, conn, %{}}

      true ->
        case session_principal_token(conn) do
          token when is_binary(token) and token != "" ->
            {:proxy, conn, %{"principal_token" => token}}

          _ ->
            {:proxy, conn, %{}}
        end
    end
  end

  defp maybe_prepare_mcp_authorize(conn, _upstream_path), do: {:proxy, conn, %{}}

  defp upstream_query(conn, extra_query) when is_map(extra_query) do
    base =
      if blank?(conn.query_string),
        do: %{},
        else: URI.decode_query(conn.query_string)

    query_map =
      base
      |> Map.merge(extra_query)
      |> Map.drop(@mcp_oauth_browser_only_query_params)

    if map_size(query_map) == 0, do: "", else: "?" <> URI.encode_query(query_map)
  end

  defp session_principal_token(conn) do
    get_session(conn, "plasm_bearer_token") || get_session(conn, :plasm_bearer_token)
  end

  defp upstream_proxy_failure_log_level(%Req.TransportError{reason: reason})
       when reason in [
              :econnrefused,
              :econnreset,
              :closed,
              :timeout,
              :enetunreach,
              :ehostunreach,
              :nxdomain
            ],
       do: :debug

  defp upstream_proxy_failure_log_level(%Req.TransportError{}), do: :warning
  defp upstream_proxy_failure_log_level(_), do: :warning

  defp redact_sensitive_url(url) when is_binary(url) do
    case URI.parse(url) do
      %URI{query: nil} = uri ->
        URI.to_string(uri)

      %URI{query: query} = uri ->
        redacted =
          query
          |> URI.decode_query()
          |> Map.drop(["principal_token"])
          |> URI.encode_query()

        URI.to_string(%{uri | query: if(redacted == "", do: nil, else: redacted)})
    end
  end

  defp blank?(v), do: is_nil(v) or v == ""
  defp present?(v), do: is_binary(v) and String.trim(v) != ""
end
