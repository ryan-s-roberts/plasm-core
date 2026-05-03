defmodule PlasmUiCore.Web.ToolIndex do
  @moduledoc """
  Shared Tool Explorer catalog index markup (SaaS + desktop).
  """
  use Phoenix.Component

  import PlasmUiCore.Web.CoreComponents, only: [button: 1]
  import PlasmUiCore.Web.McpRegistryVisuals, only: [provider_icon: 1]

  import PlasmUiCore.Web.Shell,
    only: [
      catalog_row_class: 0,
      control_input: 1,
      doc_empty_state: 1,
      doc_page: 1,
      table_surface: 1
    ]

  import PlasmUiCore.Web.ToolExplorerShared,
    only: [
      filtered_registry_entries: 2,
      mcp_scope_query_params: 1,
      registry_row_id: 1,
      registry_row_label: 1,
      registry_row_tags: 1,
      tool_entry_path: 3
    ]

  attr(:load_state, :atom, required: true)
  attr(:load_error, :any, default: nil)
  attr(:entries, :list, default: [])
  attr(:mcp_scope, :any, default: nil)
  attr(:tool_base_path, :string, required: true)
  attr(:catalog_query, :string, default: "")
  attr(:agent_unreachable_title, :string, default: "Could not reach plasm-mcp")
  attr(:omit_catalog_title, :boolean, default: false)

  slot(:error_help, required: false)

  def tool_index_body(assigns) do
    assigns = assign_new(assigns, :mcp_scope, fn -> nil end)

    ~H"""
    <.doc_page>
      <%= case @load_state do %>
        <% :loading -> %>
          <p class="text-sm text-white/62">Loading registry…</p>
        <% :error -> %>
          <div class="rounded-xl border border-error/30 bg-error/5 p-6 text-sm text-error">
            <p class="font-semibold">{@agent_unreachable_title}</p>
            <p class="mt-2 opacity-90">{@load_error}</p>
            {render_slot(@error_help)}
          </div>
        <% :ok -> %>
          <.table_surface>
            <% filtered = filtered_registry_entries(@entries, @catalog_query) %>
            <header class="border-b border-white/[0.08] px-5 py-5 sm:px-6 sm:py-6">
              <div class={
                if @omit_catalog_title do
                  "flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between"
                else
                  "flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between"
                end
              }>
                <%= if @omit_catalog_title do %>
                  <p class="text-xs text-white/55 tabular-nums sm:pt-1">
                    {length(@entries)} APIs total <span class="text-white/25">·</span>
                    {length(filtered)} visible
                  </p>
                <% else %>
                  <div>
                    <p class="text-[11px] font-semibold uppercase tracking-[0.16em] text-white/55">
                      Tool Explorer
                    </p>
                    <p class="mt-1 text-sm leading-relaxed text-white/82">
                      <%= if @mcp_scope do %>
                        Scoped to MCP Bundle
                        <span class="font-semibold text-white/92">{@mcp_scope.name}</span>
                      <% else %>
                        Full capability catalog
                      <% end %>
                    </p>
                    <p class="mt-2 text-xs text-white/55">
                      {length(@entries)} APIs total <span class="text-white/25">·</span>
                      {length(filtered)} visible
                    </p>
                  </div>
                <% end %>
                <div class={
                  if @omit_catalog_title do
                    "w-full sm:w-auto sm:min-w-[18rem]"
                  else
                    "w-full space-y-2 sm:w-auto sm:min-w-[18rem]"
                  end
                }>
                  <form
                    class="w-full"
                    phx-change="tool_catalog_search"
                    id="tool-catalog-search"
                  >
                    <label for="tool-catalog-q" class="sr-only">Search APIs</label>
                    <.control_input
                      id="tool-catalog-q"
                      type="search"
                      name="q"
                      value={@catalog_query}
                      phx-debounce="250"
                      autocomplete="off"
                      placeholder="Search by name, id, or tag…"
                    />
                  </form>
                  <%= if not @omit_catalog_title do %>
                    <.button :if={@mcp_scope} navigate={@tool_base_path} variant="secondary" size="sm">
                      View full capability catalog
                    </.button>
                  <% end %>
                </div>
              </div>
            </header>

            <div class="px-3 py-3 sm:px-4">
              <%= if @entries == [] do %>
                <.doc_empty_state
                  title="No registry entries"
                  description="The agent returned an empty registry — check your registry and session credentials."
                />
              <% else %>
                <%= if filtered == [] do %>
                  <p class="text-sm text-white/62">No APIs match “{@catalog_query}”.</p>
                <% else %>
                  <ul class="space-y-2">
                    <%= for e <- filtered do %>
                      <% eid = registry_row_id(e) %>
                      <% elab = registry_row_label(e) %>
                      <% tags = registry_row_tags(e) %>
                      <li>
                        <.link
                          navigate={
                            tool_entry_path(
                              @tool_base_path,
                              eid,
                              mcp_scope_query_params(@mcp_scope)
                            )
                          }
                          class={[
                            catalog_row_class(),
                            "group flex w-full flex-col gap-2 rounded-md border border-white/[0.08] bg-white/[0.03] px-3 py-2.5 text-left"
                          ]}
                        >
                          <div class="flex items-start gap-3">
                            <.provider_icon
                              entry_id={eid}
                              label={elab}
                              size={:md}
                              dom_id={"tool-grid-#{eid}"}
                            />
                            <div class="min-w-0 flex-1">
                              <p class="font-semibold text-white/88 group-hover:text-white">
                                {elab || eid}
                              </p>
                              <p class="mt-0.5 font-mono text-[11px] text-white/62">{eid}</p>
                            </div>
                            <span class="shrink-0 text-[10px] font-semibold uppercase tracking-wide text-[oklch(0.42_0.1_264)] opacity-0 transition group-hover:opacity-100">
                              Open →
                            </span>
                          </div>
                          <%= if tags != [] do %>
                            <div class="flex flex-wrap gap-1.5">
                              <%= for t <- Enum.take(tags, 6) do %>
                                <span class="rounded-sm bg-white/[0.05] px-2 py-0.5 text-[10px] font-medium text-white/50">
                                  {t}
                                </span>
                              <% end %>
                            </div>
                          <% end %>
                        </.link>
                      </li>
                    <% end %>
                  </ul>
                <% end %>
              <% end %>
            </div>
          </.table_surface>
      <% end %>
    </.doc_page>
    """
  end
end
