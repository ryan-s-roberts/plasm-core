defmodule PlasmUiCore.Web.ToolShow do
  @moduledoc """
  Shared per-catalog tool explorer markup (SaaS + desktop).
  """
  use Phoenix.Component

  import PlasmUiCore.Web.CoreComponents, only: [button: 1]
  import PlasmUiCore.Web.McpRegistryVisuals, only: [provider_icon: 1]

  import PlasmUiCore.Web.Shell, only: [control_input: 1, doc_page: 1, table_surface: 1]

  import PlasmUiCore.Web.ToolExplorerShared,
    only: [
      capability_line_kind_label: 1,
      catalog_path: 2,
      explorer_entity_link_class: 0,
      explorer_mono_entity_link_class: 0,
      explorer_param_type_static_class: 0,
      explorer_verb_shows_capability_id?: 1,
      filtered_entities: 2,
      ordered_capability_groups: 1,
      present?: 1,
      present_args?: 1,
      tool_entity_href: 4,
      verb_kind_label: 1,
      verb_kind_title: 1
    ]

  attr(:load_state, :atom, required: true)
  attr(:load_error, :any, default: nil)
  attr(:model, :map, default: nil)
  attr(:selected, :any, default: nil)
  attr(:url_focus, :any, default: nil)
  attr(:tool_base_path, :string, required: true)
  attr(:entry_id, :string, required: true)
  attr(:catalog_query_params, :map, required: true)
  attr(:entity_sidebar_query, :string, default: "")

  def tool_show_body(assigns) do
    ~H"""
    <%= if @load_state == :loading do %>
      <div class="py-10 text-sm text-white/62">Loading tool model…</div>
    <% end %>

    <%= if @load_state == :error do %>
      <div class="rounded-xl border border-error/30 bg-error/5 p-6 text-sm text-error">
        <p class="font-semibold">Could not load tool model</p>
        <p class="mt-2 opacity-90">{@load_error}</p>
        <.button
          variant="quiet"
          class="mt-4"
          navigate={catalog_path(@tool_base_path, @catalog_query_params)}
        >
          ← Catalog
        </.button>
      </div>
    <% end %>

    <%= if @load_state == :ok and @model do %>
      <% m = @model %>
      <% ov = m["overview"] || %{} %>
      <% detail = Enum.find(m["entities"] || [], &(&1["name"] == @selected)) %>
      <% ents = m["entities"] || [] %>
      <% filtered_ents = filtered_entities(ents, @entity_sidebar_query) %>

      <.doc_page>
        <.table_surface>
          <header class="border-b border-white/[0.08] px-5 py-5 sm:px-6 sm:py-6">
            <div class="flex flex-wrap items-start gap-5">
              <.provider_icon
                entry_id={@entry_id}
                label={m["entry"]["label"]}
                size={:lg}
                dom_id={"tool-show-head-#{@entry_id}"}
              />
              <div class="min-w-0 flex-1">
                <p class="text-[11px] font-semibold uppercase tracking-[0.16em] text-white/55">
                  {m["entry"]["entry_id"]}
                </p>
                <p class="mt-1 text-sm leading-relaxed text-white/72">
                  Tool Explorer <span class="font-semibold text-white/90">full catalog</span>
                  <span class="text-white/25"> · </span>
                  viewing <span class="font-semibold text-white/90">{@selected || "—"}</span>
                  <%= if present?(@url_focus) and @url_focus != "all" do %>
                    <span class="text-white/25"> · </span>
                    focus <span class="font-semibold text-white/90">{@url_focus}</span>
                  <% end %>
                </p>
                <p class="mt-2 text-xs text-white/55">
                  {ov["entity_count"]} entities <span class="text-white/20"> · </span>
                  {ov["relation_edge_count"]} graph edges <span class="text-white/20"> · </span>
                  {ov["verb_count"]} verbs
                </p>
              </div>
            </div>
          </header>

          <div class="grid min-h-[min(70vh,56rem)] grid-cols-1 lg:grid-cols-[minmax(12.5rem,16.5rem)_1fr]">
            <aside class="border-b border-white/[0.08] bg-white/[0.02] lg:border-b-0 lg:border-r lg:border-white/[0.08]">
              <div class="sticky top-0 flex flex-col gap-4 p-4 sm:p-5">
                <.button
                  variant="quiet"
                  class="justify-start px-0 text-sm"
                  navigate={catalog_path(@tool_base_path, @catalog_query_params)}
                >
                  ← All APIs
                </.button>
                <.button
                  type="button"
                  variant="secondary"
                  class="w-full justify-start py-2.5 text-left text-xs font-semibold"
                  phx-click="show_all"
                >
                  Full graph
                </.button>
                <div class="space-y-2">
                  <label
                    for="tool-entity-sidebar-q"
                    class="text-[10px] font-bold uppercase tracking-[0.18em] text-white/38"
                  >
                    Entities
                  </label>
                  <form
                    phx-change="entity_sidebar_search"
                    id="tool-entity-sidebar-search"
                    class="w-full"
                  >
                    <.control_input
                      id="tool-entity-sidebar-q"
                      type="search"
                      name="q"
                      value={@entity_sidebar_query}
                      phx-debounce="200"
                      autocomplete="off"
                      placeholder="Filter…"
                    />
                  </form>
                </div>
                <nav
                  class="flex max-h-[min(60vh,32rem)] flex-col gap-1 overflow-y-auto pr-1"
                  aria-label="Entity list"
                >
                  <%= if filtered_ents == [] do %>
                    <p class="py-2 text-xs text-white/62">No matching entities.</p>
                  <% else %>
                    <%= for ent <- filtered_ents do %>
                      <% ename = ent["name"] %>
                      <button
                        type="button"
                        phx-click="select_entity"
                        phx-value-name={ename}
                        class={[
                          "group flex w-full items-center gap-2.5 rounded-md border px-2.5 py-2 text-left text-sm transition",
                          ename == @selected &&
                            "border-white/[0.22] bg-white/[0.08] font-medium text-white/[0.94]",
                          ename != @selected &&
                            "border-transparent text-white/60 hover:border-white/[0.12] hover:bg-white/[0.05] hover:text-white/90"
                        ]}
                      >
                        <.provider_icon
                          entry_id={@entry_id}
                          label={ename}
                          size={:xs}
                          dom_id={"tool-sb-#{:erlang.phash2(ename)}"}
                        />
                        <span class="min-w-0 flex-1 truncate font-mono text-[13px]">{ename}</span>
                      </button>
                    <% end %>
                  <% end %>
                </nav>
              </div>
            </aside>

            <section class="min-w-0 bg-transparent">
              <%= if detail do %>
                <article class="px-5 py-6 sm:px-8 sm:py-8 lg:max-w-[46rem]">
                  <h2 class="text-[1.35rem] font-semibold leading-snug tracking-tight text-white/[0.92]">
                    {detail["name"]}
                  </h2>
                  <%= if present?(detail["description"]) do %>
                    <p class="mt-2 text-sm leading-relaxed text-white/50">
                      {detail["description"]}
                    </p>
                  <% end %>

                  <section class="mt-10">
                    <h3 class="text-[11px] font-semibold uppercase tracking-[0.2em] text-white/58">
                      Projection
                    </h3>
                    <%= if Enum.any?(detail["projection"] || []) do %>
                      <div class="mt-5 space-y-2">
                        <%= for f <- detail["projection"] do %>
                          <div class="overflow-hidden rounded-md border border-white/[0.08] bg-white/[0.03]">
                            <div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b border-white/[0.06] px-4 py-3 sm:px-5">
                              <span class="text-sm font-semibold tracking-tight text-white/[0.88]">
                                {f["name"]}
                              </span>
                              <div class="flex shrink-0 flex-wrap items-center justify-end gap-2">
                                <%= if present?(f["type_label"]) do %>
                                  <%= if f["ref_entity_navigable"] && f["ref_entity"] do %>
                                    <.link
                                      navigate={
                                        tool_entity_href(
                                          @tool_base_path,
                                          @entry_id,
                                          f["ref_entity"],
                                          @catalog_query_params
                                        )
                                      }
                                      class="rounded-md border border-[oklch(0.42_0.08_250)]/25 bg-white/[0.06] px-2.5 py-1 font-mono text-[11px] leading-tight text-[oklch(0.78_0.08_250)] transition hover:border-[oklch(0.45_0.1_250)]/45 hover:bg-white/[0.08]"
                                    >
                                      {f["type_label"]}
                                    </.link>
                                  <% else %>
                                    <span class="rounded-md border border-white/[0.08] bg-white/[0.05] px-2.5 py-1 font-mono text-[11px] leading-tight text-white/60">
                                      {f["type_label"]}
                                    </span>
                                  <% end %>
                                <% end %>
                              </div>
                            </div>
                            <%= if present?(f["description"]) do %>
                              <div class="border-t border-white/[0.06] px-4 py-2.5 sm:px-5">
                                <p class="text-xs leading-relaxed text-white/62">
                                  {f["description"]}
                                </p>
                              </div>
                            <% end %>
                          </div>
                        <% end %>
                      </div>
                    <% else %>
                      <p class="mt-4 text-sm text-white/62">No declared fields.</p>
                    <% end %>
                  </section>

                  <section class="mt-10">
                    <h3 class="text-[11px] font-semibold uppercase tracking-[0.2em] text-white/58">
                      Capabilities
                    </h3>
                    <%= if Enum.any?(detail["capabilities"] || []) do %>
                      <div class="mt-4 space-y-8">
                        <%= for {kind, rows} <-
                              ordered_capability_groups(
                                Enum.group_by(detail["capabilities"] || [], & &1["line_kind"])
                              ) do %>
                          <div>
                            <h4 class="text-[10px] font-bold uppercase tracking-[0.22em] text-white/35">
                              {capability_line_kind_label(kind)}
                            </h4>
                            <ul class="mt-3 space-y-4">
                              <%= for row <- rows do %>
                                <li class="rounded-md border border-white/[0.1] bg-white/[0.03] px-4 py-3">
                                  <%= if present?(row["description"]) do %>
                                    <p class="mb-3 text-xs leading-relaxed text-white/50">
                                      {row["description"]}
                                    </p>
                                  <% end %>
                                  <pre class="min-w-0 overflow-x-auto whitespace-pre-wrap font-mono text-xs leading-relaxed text-white/82">{row["expression"]}</pre>
                                  <%= if present?(row["capability_name"]) do %>
                                    <p class="mt-1.5 font-mono text-[10px] tracking-wide text-white/35">
                                      <span class="text-white/20">Capability · </span>
                                      {row["capability_name"]}
                                    </p>
                                  <% end %>
                                  <% ret = row["returns"] || %{} %>
                                  <p class="mt-2 text-[11px] text-white/62">
                                    <span class="text-white/30">→</span>
                                    <%= if ret["entity_navigable"] && ret["entity"] do %>
                                      <.link
                                        navigate={
                                          tool_entity_href(
                                            @tool_base_path,
                                            @entry_id,
                                            ret["entity"],
                                            @catalog_query_params
                                          )
                                        }
                                        class={explorer_entity_link_class()}
                                      >
                                        {ret["label"]}
                                      </.link>
                                    <% else %>
                                      <span class="text-white/60">{ret["label"] || "—"}</span>
                                    <% end %>
                                    <span class="ml-2 font-mono text-[10px] text-white/25">
                                      :{ret["kind"] || "?"}
                                    </span>
                                  </p>
                                  <%= if present?(ret["description"]) do %>
                                    <p class="mt-2 text-[11px] leading-snug text-white/62">
                                      {ret["description"]}
                                    </p>
                                  <% end %>
                                  <%= if present_args?(row["parameters"]) do %>
                                    <ul class="mt-3 space-y-1.5 border-t border-white/[0.06] pt-3 text-[11px] text-white/50">
                                      <%= for a <- row["parameters"] do %>
                                        <li class="space-y-1">
                                          <div class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                                            <span class="shrink-0 font-mono text-white/72">
                                              {a["binding"]}
                                            </span>
                                            <%= if present?(a["type_label"]) do %>
                                              <%= if a["ref_entity"] do %>
                                                <.link
                                                  navigate={
                                                    tool_entity_href(
                                                      @tool_base_path,
                                                      @entry_id,
                                                      a["ref_entity"],
                                                      @catalog_query_params
                                                    )
                                                  }
                                                  class={explorer_mono_entity_link_class()}
                                                >
                                                  {a["type_label"]}
                                                </.link>
                                              <% else %>
                                                <span class={explorer_param_type_static_class()}>
                                                  {a["type_label"]}
                                                </span>
                                              <% end %>
                                            <% end %>
                                            <span class="text-white/25">{a["role"]}</span>
                                            <%= if is_boolean(a["required"]) do %>
                                              <span class="text-white/20">
                                                {if a["required"], do: "required", else: "optional"}
                                              </span>
                                            <% end %>
                                          </div>
                                          <%= if present?(a["description"]) do %>
                                            <p class="pl-0 text-[11px] leading-snug text-white/62">
                                              {a["description"]}
                                            </p>
                                          <% end %>
                                        </li>
                                      <% end %>
                                    </ul>
                                  <% end %>
                                </li>
                              <% end %>
                            </ul>
                          </div>
                        <% end %>
                      </div>
                    <% else %>
                      <ul class="mt-4 space-y-3">
                        <%= for v <- detail["verbs"] || [] do %>
                          <li class="rounded-md border border-white/[0.1] bg-white/[0.03] px-4 py-3">
                            <div class="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-x-3 gap-y-1">
                              <span class="min-w-0 font-mono text-sm text-white/75">
                                {v["label"]}
                              </span>
                              <span
                                class="shrink-0 rounded bg-black/[0.08] px-2 py-0.5 text-[10px] uppercase tracking-wide text-white/62"
                                title={verb_kind_title(v["kind"])}
                              >
                                {verb_kind_label(v["kind"])}
                              </span>
                              <div class="col-span-2 min-w-0">
                                <%= if explorer_verb_shows_capability_id?(v["kind"]) && present?(v["capability_name"]) do %>
                                  <p class="mb-1 font-mono text-[10px] tracking-wide text-white/30">
                                    <span class="text-white/20">Capability · </span>
                                    {v["capability_name"]}
                                  </p>
                                <% end %>
                                <p class="text-xs text-white/62">{v["about"]}</p>
                                <% ret = v["returns"] || %{} %>
                                <p class="mt-1 text-[11px] text-white/62">
                                  <span class="text-white/30">→</span>
                                  <%= if ret["entity_navigable"] && ret["entity"] do %>
                                    <.link
                                      navigate={
                                        tool_entity_href(
                                          @tool_base_path,
                                          @entry_id,
                                          ret["entity"],
                                          @catalog_query_params
                                        )
                                      }
                                      class={explorer_entity_link_class()}
                                    >
                                      {ret["label"]}
                                    </.link>
                                  <% else %>
                                    <span>{ret["label"] || "—"}</span>
                                  <% end %>
                                  <span class="ml-2 font-mono text-[10px] text-white/25">
                                    :{ret["kind"] || "?"}
                                  </span>
                                </p>
                                <%= if present?(ret["description"]) do %>
                                  <p class="mt-2 text-[11px] leading-snug text-white/62">
                                    {ret["description"]}
                                  </p>
                                <% end %>
                                <%= if present_args?(v["arguments"]) do %>
                                  <ul class="mt-2 space-y-1 border-t border-white/[0.06] pt-2 text-[11px] text-white/50">
                                    <%= for a <- v["arguments"] do %>
                                      <li class="space-y-1">
                                        <div class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                                          <span class="shrink-0 font-mono text-white/72">
                                            {a["binding"]}
                                          </span>
                                          <%= if present?(a["type_label"]) do %>
                                            <%= if a["ref_entity"] do %>
                                              <.link
                                                navigate={
                                                  tool_entity_href(
                                                    @tool_base_path,
                                                    @entry_id,
                                                    a["ref_entity"],
                                                    @catalog_query_params
                                                  )
                                                }
                                                class={explorer_mono_entity_link_class()}
                                              >
                                                {a["type_label"]}
                                              </.link>
                                            <% else %>
                                              <span class={explorer_param_type_static_class()}>
                                                {a["type_label"]}
                                              </span>
                                            <% end %>
                                          <% end %>
                                        </div>
                                        <%= if present?(a["description"]) do %>
                                          <p class="text-[11px] text-white/62">{a["description"]}</p>
                                        <% end %>
                                      </li>
                                    <% end %>
                                  </ul>
                                <% end %>
                              </div>
                            </div>
                          </li>
                        <% end %>
                      </ul>
                    <% end %>
                  </section>

                  <%= if Enum.any?(detail["relations"] || []) do %>
                    <section class="mt-10">
                      <h3 class="text-[11px] font-bold uppercase tracking-[0.2em] text-white/58">
                        Relations
                      </h3>
                      <ul class="mt-4 space-y-3">
                        <%= for r <- detail["relations"] || [] do %>
                          <li class="rounded-md border border-white/[0.1] bg-white/[0.03] px-4 py-3">
                            <p class="font-mono text-sm text-white/80">
                              {r["name"]} →
                              <%= if r["target_entity_navigable"] && r["target_entity"] do %>
                                <.link
                                  navigate={
                                    tool_entity_href(
                                      @tool_base_path,
                                      @entry_id,
                                      r["target_entity"],
                                      @catalog_query_params
                                    )
                                  }
                                  class={explorer_entity_link_class()}
                                >
                                  {r["target_entity"]}
                                </.link>
                              <% else %>
                                {r["target_entity"]}
                              <% end %>
                              <span class="ml-2 text-xs text-white/35">({r["cardinality"]})</span>
                            </p>
                            <%= if present?(r["about"]) do %>
                              <p class="mt-1 text-xs text-white/62">{r["about"]}</p>
                            <% end %>
                          </li>
                        <% end %>
                      </ul>
                    </section>
                  <% end %>

                  <%= if Enum.any?(detail["reverse_traversals"] || []) do %>
                    <section class="mt-10">
                      <h3 class="text-[11px] font-bold uppercase tracking-[0.2em] text-white/58">
                        Reverse traversals
                      </h3>
                      <ul class="mt-4 space-y-2 text-sm">
                        <%= for r <- detail["reverse_traversals"] || [] do %>
                          <li class="rounded-lg border border-dashed border-white/15 px-3 py-2 text-white/65">
                            <%= if r["source_entity_navigable"] && r["source_entity"] do %>
                              <.link
                                navigate={
                                  tool_entity_href(
                                    @tool_base_path,
                                    @entry_id,
                                    r["source_entity"],
                                    @catalog_query_params
                                  )
                                }
                                class={explorer_mono_entity_link_class()}
                              >
                                {r["subcommand"]}
                              </.link>
                            <% else %>
                              <span class="font-mono">{r["subcommand"]}</span>
                            <% end %>
                            <span class="text-white/35">
                              · via {r["via_param"]} on
                            </span>
                            <%= if r["source_entity_navigable"] && r["source_entity"] do %>
                              <.link
                                navigate={
                                  tool_entity_href(
                                    @tool_base_path,
                                    @entry_id,
                                    r["source_entity"],
                                    @catalog_query_params
                                  )
                                }
                                class={explorer_entity_link_class()}
                              >
                                {r["source_entity"]}
                              </.link>
                            <% else %>
                              <span>{r["source_entity"]}</span>
                            <% end %>
                            <%= if present?(r["about"]) do %>
                              <p class="mt-2 text-xs leading-relaxed text-white/62">{r["about"]}</p>
                            <% end %>
                          </li>
                        <% end %>
                      </ul>
                    </section>
                  <% end %>

                  <%= if Enum.any?(detail["entity_ref_links"] || []) do %>
                    <section class="mt-10">
                      <h3 class="text-[11px] font-bold uppercase tracking-[0.2em] text-white/58">
                        EntityRef links
                      </h3>
                      <ul class="mt-4 space-y-2 text-sm text-white/60">
                        <%= for r <- detail["entity_ref_links"] || [] do %>
                          <li class="rounded-lg border border-white/[0.06] bg-black/[0.02] px-3 py-2">
                            <span class="font-mono text-white/72">{r["field"]}</span>
                            →
                            <%= if r["target_entity_navigable"] && r["target_entity"] do %>
                              <.link
                                navigate={
                                  tool_entity_href(
                                    @tool_base_path,
                                    @entry_id,
                                    r["target_entity"],
                                    @catalog_query_params
                                  )
                                }
                                class={explorer_entity_link_class()}
                              >
                                {r["target_entity"]}
                              </.link>
                            <% else %>
                              {r["target_entity"]}
                            <% end %>
                            <%= if present?(r["description"]) do %>
                              <p class="mt-2 text-xs leading-relaxed text-white/62">
                                {r["description"]}
                              </p>
                            <% end %>
                          </li>
                        <% end %>
                      </ul>
                    </section>
                  <% end %>
                </article>
              <% else %>
                <p class="px-5 py-8 text-sm text-white/62 sm:px-8">Select an entity.</p>
              <% end %>
            </section>
          </div>
        </.table_surface>
      </.doc_page>
    <% end %>
    """
  end
end
