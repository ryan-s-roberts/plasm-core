defmodule PlasmDesktop.Mcp.Config do
  @moduledoc """
  Resolves agent HTTP base, public MCP URL, and related session keys for the appliance.

  Precedence: Plug session (merged from env + `desktop_settings`) → inline env → defaults.
  """

  @spec resolve_http_base(map()) :: String.t()
  def resolve_http_base(session) when is_map(session) do
    session["plasm_mcp_http_base_url"] || session[:plasm_mcp_http_base_url] ||
      nonempty(System.get_env("PLASM_MCP_HTTP_BASE_URL")) || "http://127.0.0.1:3000"
    |> trim_slash()
  end

  @spec resolve_public_base(map()) :: String.t()
  def resolve_public_base(session) when is_map(session) do
    session["plasm_mcp_public_base_url"] || session[:plasm_mcp_public_base_url] ||
      nonempty(System.get_env("PLASM_MCP_PUBLIC_BASE_URL")) ||
      "http://127.0.0.1:3001/mcp"
    |> trim_slash()
  end

  @spec resolve_mcp_upstream(map()) :: String.t()
  def resolve_mcp_upstream(session) when is_map(session), do: resolve_public_base(session)

  defp nonempty(v) when is_binary(v) do
    t = String.trim(v)
    if t == "", do: nil, else: t
  end

  defp nonempty(_), do: nil

  defp trim_slash(url) when is_binary(url) do
    url |> String.trim() |> String.trim_trailing("/")
  end
end
