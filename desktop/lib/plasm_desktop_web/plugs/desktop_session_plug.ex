defmodule PlasmDesktopWeb.DesktopSessionPlug do
  @moduledoc """
  Merges env + `desktop_settings` into the Plug session so LiveViews resolve agent URLs and auth headers.

  Precedence per key: **`System.get_env/1` first**, then saved desktop setting, then (implicit) defaults inside
  [`PlasmDesktop.Mcp.Config`](`PlasmDesktop.Mcp.Config`).
  """

  @behaviour Plug

  import Plug.Conn

  alias PlasmDesktop.Settings

  @session_keys [
    {"PLASM_MCP_HTTP_BASE_URL", "plasm_mcp_http_base_url"},
    {"PLASM_MCP_PUBLIC_BASE_URL", "plasm_mcp_public_base_url"},
    {"PLASM_DESKTOP_BEARER_TOKEN", "plasm_bearer_token"},
    {"PLASM_DESKTOP_API_KEY", "plasm_api_key"}
  ]

  def init(opts), do: opts

  def call(conn, _opts) do
    db = Settings.get_all_map()

    Enum.reduce(@session_keys, conn, fn {env_k, sess_k}, acc ->
      case first_present([System.get_env(env_k), Map.get(db, sess_k)]) do
        nil -> acc
        v -> put_session(acc, sess_k, v)
      end
    end)
  end

  defp first_present(list) do
    Enum.find_value(list, fn
      v when is_binary(v) ->
        t = String.trim(v)
        if t == "", do: nil, else: t

      _ ->
        nil
    end)
  end
end
