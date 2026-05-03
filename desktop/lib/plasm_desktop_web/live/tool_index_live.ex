defmodule PlasmDesktopWeb.ToolIndexLive do
  @moduledoc """
  Catalog index from `GET /v1/registry` (appliance tool explorer).
  """
  use PlasmDesktopWeb, :live_view

  import PlasmUiCore.Web.ToolIndex, only: [tool_index_body: 1]

  alias PlasmDesktopWeb.ToolExplorerShared

  @impl true
  def mount(_params, session, socket) do
    socket =
      socket
      |> assign(:page_title, "Tool catalog")
      |> assign(:session_plug, session)
      |> assign(:tool_base_path, "/tools")
      |> assign(:entries, [])
      |> assign(:load_state, :loading)
      |> assign(:load_error, nil)
      |> assign(:catalog_query, "")

    case PlasmDesktop.Mcp.DataPlane.list_registry(session) do
      {:ok, body} when is_map(body) ->
        entries =
          Map.get(body, "entries") ||
            Map.get(body, :entries) ||
            []

        if is_list(entries) do
          {:ok, assign(socket, load_state: :ok, entries: entries)}
        else
          {:ok,
           assign(socket,
             load_state: :error,
             load_error:
               "unexpected registry response (entries not a list): #{inspect(body, limit: 200)}"
           )}
        end

      {:ok, other} ->
        {:ok,
         assign(socket,
           load_state: :error,
           load_error: "unexpected registry response: #{inspect(other, limit: 200)}"
         )}

      {:error, reason} ->
        {:ok,
         assign(socket,
           load_state: :error,
           load_error: PlasmDesktop.Mcp.DataPlane.format_agent_http_error(session, reason)
         )}
    end
  end

  @impl true
  def handle_event("tool_catalog_search", params, socket) do
    q = ToolExplorerShared.parse_search_param(params, "q")
    {:noreply, assign(socket, :catalog_query, q)}
  end

  @impl true
  def render(assigns) do
    ~H"""
    <div class="plasm-doc-stack">
      <.page_header
        eyebrow="Explore"
        title="Tool catalog"
        subtitle="Registry entries from this agent — open one for DOMAIN detail."
      >
        <:actions>
          <a class="plasm-button plasm-button-secondary" href="/connect-apis">Connect APIs</a>
          <a class="plasm-button plasm-button-ghost" href="/">Station home</a>
        </:actions>
      </.page_header>

      <.tool_index_body
        load_state={@load_state}
        load_error={@load_error}
        entries={@entries}
        tool_base_path={@tool_base_path}
        catalog_query={@catalog_query}
        agent_unreachable_title="Could not reach the agent"
        omit_catalog_title
      >
      <:error_help>
        <p class="m-0 mt-3 text-xs leading-relaxed text-white/55">
          Set values under <a class="font-medium underline" href="/settings">Settings</a> or export
          <code class="rounded-md border border-white/10 bg-black/[0.04] px-1.5 py-0.5 font-mono text-[11px]">
            PLASM_MCP_HTTP_BASE_URL
          </code>
          /
          <code class="rounded-md border border-white/10 bg-black/[0.04] px-1.5 py-0.5 font-mono text-[11px]">
            PLASM_DESKTOP_BEARER_TOKEN
          </code>
          .
        </p>
      </:error_help>
    </.tool_index_body>
    </div>
    """
  end
end
