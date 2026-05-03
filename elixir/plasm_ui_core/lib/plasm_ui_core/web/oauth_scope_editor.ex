defmodule PlasmUiCore.Web.OauthScopeEditor do
  @moduledoc false
  use Phoenix.Component

  import PlasmUiCore.Web.CoreComponents, only: [button: 1]
  import PlasmUiCore.Web.Shell, only: [control_input: 1, icon_action_button: 1]

  attr(:profile, :map, default: nil)
  attr(:scope_draft, :list, default: [])
  attr(:scope_query, :string, default: "")
  attr(:query_event, :string, required: true)
  attr(:add_event, :string, required: true)
  attr(:remove_event, :string, required: true)
  attr(:apply_set_event, :string, required: true)
  attr(:query_input_id, :string, default: "oauth-scope-query")
  attr(:save_event, :string, default: nil)
  attr(:save_label, :string, default: "Save scopes")

  def oauth_scope_editor(assigns) do
    catalog = if is_map(assigns.profile), do: assigns.profile["scope_entries"] || %{}, else: %{}

    default_sets =
      if is_map(assigns.profile), do: assigns.profile["default_scope_sets"] || %{}, else: %{}

    catalog_size = map_size(catalog)
    suggestions = filtered_scope_suggestions(catalog, assigns.scope_query, assigns.scope_draft)

    assigns =
      assign(assigns,
        catalog: catalog,
        default_sets: default_sets,
        catalog_size: catalog_size,
        suggestions: suggestions
      )

    ~H"""
    <div class="space-y-4">
      <div class="rounded-lg border border-white/[0.08] bg-black/[0.03] px-3 py-3">
        <p class="text-xs font-semibold text-white/65">Why scopes matter</p>
        <p class="mt-1 text-xs leading-relaxed text-white/60">
          OAuth scopes define exactly what this app can read or change in your external account. Pick the minimum needed for your workflow.
        </p>
      </div>

      <%= if @catalog_size == 0 do %>
        <p class="rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-950">
          No OAuth scope catalog in CGS for this app. Add <code class="text-xs">oauth.scopes</code>
          in domain.yaml.
        </p>
      <% else %>
        <%= if map_size(@default_sets) > 0 do %>
          <div>
            <p class="mb-2 text-xs font-semibold text-white/60">Default scope sets</p>
            <div class="flex flex-wrap gap-2">
              <%= for {name, _} <- Enum.sort_by(@default_sets, fn {k, _} -> k end) do %>
                <.button
                  type="button"
                  phx-click={@apply_set_event}
                  phx-value-set={name}
                  variant="secondary"
                  size="xs"
                >
                  {name}
                </.button>
              <% end %>
            </div>
          </div>
        <% end %>

        <form phx-change={@query_event} class="relative">
          <label class="sr-only" for={@query_input_id}>Find scope</label>
          <.control_input
            type="text"
            id={@query_input_id}
            name="q"
            value={@scope_query}
            phx-debounce="120"
            placeholder="Search scopes from CGS catalog..."
            autocomplete="off"
            class="pr-10 py-2.5"
          />
          <span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-white/35 text-lg">
            ↵
          </span>
        </form>

        <%= if @suggestions != [] do %>
          <ul class="max-h-48 overflow-y-auto rounded-lg border border-white/[0.08] bg-black/[0.03]">
            <%= for scope <- @suggestions do %>
              <% spec = scope_spec(@catalog, scope) %>
              <li class="border-b border-white/[0.06] last:border-0">
                <button
                  type="button"
                  phx-click={@add_event}
                  phx-value-scope={scope}
                  class="flex w-full items-start gap-2 px-3 py-2 text-left text-sm hover:bg-white/[0.06]"
                >
                  <span class="flex-1 font-mono text-xs text-white/85 break-all">{scope}</span>
                  <span
                    :if={is_map(spec) and spec["label"] not in [nil, ""]}
                    class="text-xs text-white/62"
                  >
                    {spec["label"]}
                  </span>
                </button>
              </li>
            <% end %>
          </ul>
        <% end %>
      <% end %>

      <div>
        <p class="mb-2 text-xs font-semibold text-white/60">Selected scopes</p>
        <%= if @scope_draft == [] do %>
          <p class="text-sm text-white/62">No scopes selected yet.</p>
        <% else %>
          <ul class="divide-y divide-white/[0.06] rounded-lg border border-white/[0.08] bg-white/[0.06]">
            <%= for scope <- @scope_draft do %>
              <% spec = scope_spec(@catalog, scope) %>
              <li class="flex items-center gap-3 px-3 py-2.5">
                <div class="min-w-0 flex-1">
                  <p class="font-mono text-xs text-white/90 break-all">{scope}</p>
                  <p
                    :if={is_map(spec) and spec["label"] not in [nil, ""]}
                    class="text-xs text-white/62"
                  >
                    {spec["label"]}
                  </p>
                </div>
                <.icon_action_button
                  icon="hero-x-mark"
                  label="Remove scope"
                  tone={:danger}
                  size={:sm}
                  phx-click={@remove_event}
                  phx-value-scope={scope}
                />
              </li>
            <% end %>
          </ul>
        <% end %>
      </div>

      <div class="rounded-lg border border-white/[0.06] bg-black/[0.03] px-3 py-3">
        <p class="text-xs font-semibold text-white/65">Scope reference</p>
        <ul class="mt-2 space-y-2 text-xs text-white/55">
          <%= for scope <- Enum.take(@scope_draft, 8) do %>
            <% spec = scope_spec(@catalog, scope) %>
            <li>
              <span class="font-mono text-[11px] text-white/85">{scope}</span>
              <span :if={is_map(spec) and spec["description"] not in [nil, ""]} class="ml-2">
                {spec["description"]}
              </span>
              <span :if={is_map(spec) and spec["docs_url"]} class="ml-2">
                <a
                  href={spec["docs_url"]}
                  target="_blank"
                  rel="noopener noreferrer"
                  class="text-sky-300 underline decoration-sky-400/60 underline-offset-2 hover:text-sky-200"
                >
                  Documentation
                </a>
              </span>
            </li>
          <% end %>
        </ul>
      </div>

      <.button
        :if={is_binary(@save_event) and @save_event != ""}
        type="button"
        phx-click={@save_event}
        variant="primary"
        class="w-full min-h-11"
      >
        {@save_label}
      </.button>
    </div>
    """
  end

  defp filtered_scope_suggestions(catalog, scope_query, scope_draft) when is_map(catalog) do
    q = scope_query |> to_string() |> String.trim() |> String.downcase()
    draft = scope_draft || []

    catalog
    |> Map.keys()
    |> Enum.reject(&(&1 in draft))
    |> Enum.filter(fn key ->
      if q == "" do
        true
      else
        desc = (catalog[key] || %{})["description"] |> to_string() |> String.downcase()
        String.contains?(String.downcase(key), q) or String.contains?(desc, q)
      end
    end)
    |> Enum.sort()
    |> Enum.take(12)
  end

  defp scope_spec(catalog, scope) when is_map(catalog), do: Map.get(catalog, scope) || %{}
end
