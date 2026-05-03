defmodule PlasmUiCore.Web.CopyComponents do
  @moduledoc """
  MCP URL / JSON copy rows and key selector — shared by SaaS MCP screens.
  """
  use Phoenix.Component

  import PlasmUiCore.Web.CoreComponents, only: [icon: 1]

  alias PlasmUiCore.Web.McpJsonHighlight

  attr(:label, :string, default: "")
  attr(:value, :string, required: true)
  attr(:display_value, :string, default: nil)
  attr(:copy_id, :string, required: true)
  attr(:tone, :atom, default: :amber)

  attr(:code_format, :atom, default: nil)

  attr(:json_map, :any, default: nil)

  def copy_value_row(assigns) do
    is_json? = assigns.code_format == :json

    value_box =
      case {assigns.tone, is_json?} do
        {:light, true} ->
          "mcp-json-hl min-w-0 flex-1 overflow-x-auto whitespace-pre rounded-md border border-amber-900/20 bg-white/75 px-3 py-2.5 text-left font-mono text-[12px] leading-6 antialiased sm:text-xs"

        {:light, false} ->
          "min-w-0 flex-1 overflow-x-auto whitespace-pre rounded-md border border-amber-900/20 bg-white/75 px-3 py-2.5 text-left font-mono text-[12px] leading-6 tracking-normal text-amber-950 antialiased sm:text-xs"

        {:neutral, true} ->
          "mcp-json-hl min-w-0 flex-1 overflow-x-auto whitespace-pre rounded-md border border-white/15 bg-white/[0.06] px-3 py-2.5 text-left font-mono text-[12px] leading-6 sm:text-xs"

        {:neutral, false} ->
          "min-w-0 flex-1 overflow-x-auto whitespace-pre rounded-md border border-white/15 bg-white/[0.06] px-3 py-2.5 text-left font-mono text-[12px] leading-6 tracking-normal text-white/92 antialiased sm:text-xs"

        {_, true} ->
          "mcp-json-hl min-w-0 flex-1 overflow-x-auto whitespace-pre rounded-md bg-white/[0.08] px-3 py-2.5 text-left font-mono text-[12px] leading-6 sm:text-xs"

        _ ->
          "min-w-0 flex-1 overflow-x-auto whitespace-pre rounded-md bg-white/[0.08] px-3 py-2.5 text-left font-mono text-[12px] leading-6 tracking-normal text-amber-100 antialiased sm:text-xs"
      end

    btn =
      case assigns.tone do
        :light ->
          "shrink-0 rounded-md border border-amber-900/25 bg-white/80 px-2.5 py-1.5 text-amber-950 hover:bg-white"

        :neutral ->
          "shrink-0 rounded-md border border-white/20 bg-white/[0.08] text-white/92 hover:bg-white/[0.14]"

        _ ->
          "shrink-0 rounded-md border border-amber-400/30 bg-white/[0.06] text-amber-100 hover:bg-amber-500/20"
      end

    label_class =
      case assigns.tone do
        :light -> "font-sans text-amber-950/85"
        _ -> "font-sans text-white/50"
      end

    feedback_class =
      case assigns.tone do
        :light -> "text-emerald-800"
        _ -> "text-emerald-200"
      end

    feedback_pill =
      case assigns.tone do
        :light ->
          "border-amber-900/15 bg-white/95 ring-amber-900/10"

        _ ->
          "border-white/15 bg-slate-950/95 ring-white/5"
      end

    vbox = if is_json?, do: [value_box, "mcp-json-block"], else: [value_box]
    has_json_map? = is_json? and is_map(assigns[:json_map])

    assigns =
      assign(assigns,
        value_box_class: vbox,
        btn_class: btn,
        label_class: label_class,
        display_text: assigns.display_value || assigns.value,
        feedback_class: feedback_class,
        feedback_pill: feedback_pill,
        is_json: is_json?,
        use_json_map: has_json_map?
      )

    ~H"""
    <div class="space-y-2">
      <div :if={@label != ""} class={@label_class}>{@label}</div>
      <div
        id={@copy_id}
        phx-hook="CopyToClipboard"
        phx-update="ignore"
        class="min-w-0 flex flex-col gap-2 sm:flex-row sm:items-stretch"
        data-copy-text={@value}
      >
        <pre
          :if={@is_json}
          class={[
            @value_box_class,
            "m-0",
            "leading-relaxed text-[11px] sm:text-xs"
          ]}
        >
          {if @use_json_map,
            do: McpJsonHighlight.pretty_map(@json_map),
            else: McpJsonHighlight.safe_pretty(@display_text)}
        </pre>
        <pre :if={not @is_json} class={[@value_box_class, "m-0"]}><%= @display_text %></pre>
        <div class={["relative z-20 shrink-0 self-start", @is_json && "sm:mt-0 sm:self-start"]}>
          <span
            data-copy-feedback
            class={[
              "pointer-events-none absolute right-0 bottom-full z-30 mb-1 hidden whitespace-nowrap rounded-md border px-2 py-0.5 text-[10px] font-semibold shadow-sm ring-1",
              @feedback_pill,
              @feedback_class
            ]}
            aria-hidden="true"
          >
            Copied
          </span>
          <button
            type="button"
            data-copy-trigger
            class={[
              @btn_class,
              "inline-flex h-8 w-8 min-h-[2rem] min-w-[2rem] items-center justify-center p-0"
            ]}
            aria-label="Copy to clipboard"
          >
            <span data-copy-main class="inline-flex items-center justify-center">
              <.icon name="hero-clipboard-document" class="size-4 shrink-0" />
              <span class="sr-only">Copy</span>
            </span>
          </button>
        </div>
      </div>
    </div>
    """
  end

  attr(:config_id, :string, required: true)
  attr(:form_id, :string, required: true)
  attr(:rows, :list, default: [])
  attr(:selected_id, :string, default: "")

  def mcp_key_select(assigns) do
    ~H"""
    <div :if={@rows != []} class="mb-3 space-y-1.5">
      <p class="text-[11px] font-medium uppercase tracking-wide text-white/50">
        Key for snippets in this dialog
      </p>
      <form phx-change="mcp_api_key_select" id={@form_id} class="max-w-md">
        <input type="hidden" name="config_id" value={@config_id} />
        <label class="sr-only" for={@form_id <> "-mcp-sel"}>MCP API key (Bearer)</label>
        <select
          id={@form_id <> "-mcp-sel"}
          name="key_id"
          class="w-full rounded-md border border-white/15 bg-black/30 px-2.5 py-1.5 text-sm text-white/90"
        >
          <%= for row <- @rows do %>
            <% r_id = to_string(row["key_id"] || row[:key_id] || "") %>
            <option value={r_id} selected={r_id == to_string(@selected_id || "")}>
              {mcp_key_option_label(row)}
            </option>
          <% end %>
        </select>
      </form>
    </div>
    """
  end

  defp mcp_key_option_label(row) when is_map(row) do
    n = to_string(row["label"] || row[:label] || "") |> String.trim()
    fp = to_string(row["key_fingerprint"] || row[:key_fingerprint] || "—")
    if n != "", do: n, else: "Unnamed key · " <> fp <> "…"
  end
end
