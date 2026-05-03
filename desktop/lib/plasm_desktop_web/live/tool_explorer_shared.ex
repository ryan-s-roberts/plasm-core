defmodule PlasmDesktopWeb.ToolExplorerShared do
  @moduledoc false

  defdelegate parse_search_param(params), to: PlasmUiCore.Web.ToolExplorerShared
  defdelegate parse_search_param(params, key), to: PlasmUiCore.Web.ToolExplorerShared

  defdelegate normalize_catalog_query_params(params), to: PlasmUiCore.Web.ToolExplorerShared

  defdelegate tool_entry_path(tool_base_path, entry_id), to: PlasmUiCore.Web.ToolExplorerShared

  defdelegate tool_entry_path(tool_base_path, entry_id, query_params),
    to: PlasmUiCore.Web.ToolExplorerShared

  @doc "No-op on appliance (no SaaS project shell)."
  def assign_shell_context(socket), do: socket

  def tool_paths(_params), do: {"/tools", "/"}

  def tool_base_path_from_params(_params), do: "/tools"
end
