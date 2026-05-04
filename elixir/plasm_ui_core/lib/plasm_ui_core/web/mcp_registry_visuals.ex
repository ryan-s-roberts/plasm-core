defmodule PlasmUiCore.Web.McpRegistryVisuals do
  @moduledoc """
  Registry catalog icons: **vendored SVGs** under `plasm_ui_core` `priv/static/images/brand-icons/`
  (see `INVENTORY.md` there), otherwise an **inline SVG monogram**.

  Host apps must mount `Plug.Static` for `:plasm_ui_core` at [`PlasmUiCore.Assets.static_mount_path/0`](`PlasmUiCore.Assets.static_mount_path/0`)
  with `only: ~w(css images)` so URLs resolve.
  """

  use Phoenix.Component

  alias PlasmUiCore.Assets

  # `entry_id` (or prefix before `-`) → basename without `.svg` in `priv/static/images/brand-icons/`.
  @brand_icon_files %{
    "github" => "github",
    "linear" => "linear",
    "notion" => "notion",
    "slack" => "slack",
    "gitlab" => "gitlab",
    "gmail" => "gmail",
    "jira" => "jira",
    "clickup" => "clickup",
    "claude" => "anthropic",
    "chatgpt" => "openai",
    "codex" => "openai",
    "agent_builder" => "openai",
    "vscode" => "visualstudiocode",
    "windsurf" => "windsurf",
    "cursor" => "cursor",
    "openclaw" => "openclaw",
    "google-drive" => "googledrive",
    "google-docs" => "googledocs",
    "vultr" => "vultr"
  }

  @doc """
  Returns `{:local, basename}` or `:fallback` for unknown entry ids.

  Tries the full `entry_id` first, then the segment before the first `-` so suffixed
  catalog ids (e.g. `clickup-b24452`) still resolve to the same brand icon as `clickup`.
  """
  def simple_icon_spec(entry_id) when is_binary(entry_id) do
    id = entry_id |> String.trim() |> String.downcase()

    resolve = fn key ->
      case Map.get(@brand_icon_files, key) do
        base when is_binary(base) -> {:local, base}
        nil -> nil
      end
    end

    case resolve.(id) do
      {:local, _} = hit ->
        hit

      nil ->
        base =
          case String.split(id, "-", parts: 2) do
            [a, _] -> a
            [a] -> a
          end

        case resolve.(base) do
          {:local, _} = hit -> hit
          nil -> :fallback
        end
    end
  end

  def simple_icon_spec(_), do: :fallback

  @doc "URL path for a vendored brand SVG (under `PlasmUiCore` static mount)."
  def brand_icon_url(basename) when is_binary(basename) do
    Assets.brand_icon_href(basename)
  end

  @doc "Two-letter monogram for fallback tiles."
  def monogram(entry_id, label \\ nil)

  def monogram(entry_id, label) when is_binary(entry_id) do
    label = label || entry_id

    label
    |> String.replace(~r/[^a-zA-Z0-9\s]/u, " ")
    |> String.split()
    |> Enum.filter(&(&1 != ""))
    |> case do
      [a, b | _] ->
        (String.first(a) <> String.first(b)) |> String.upcase()

      [a | _] ->
        String.slice(String.upcase(a), 0, 2)

      _ ->
        entry_id |> String.slice(0, 2) |> String.upcase()
    end
  end

  def monogram(_, _), do: "??"

  @doc """
  Stable hue (0..360) from entry_id for fallback tile background (Plasm-tinted neutrals).
  """
  def accent_hue(entry_id) when is_binary(entry_id) do
    <<a, b, c, _::binary>> = :crypto.hash(:md5, entry_id)
    rem(a + b + c, 360)
  end

  def accent_hue(_), do: 255

  attr(:dom_id, :string, default: "plasm-brand-icon")
  attr(:class, :any, default: nil)

  @doc """
  Plasm product mark (cyan “P” icon) for outbound connect and other chrome-free surfaces.
  """
  def plasm_brand_icon(assigns) do
    src = Assets.plasm_mark_href()

    assigns = assign(assigns, :mark_src, src)

    ~H"""
    <span
      id={@dom_id}
      class={[
        "relative inline-flex shrink-0 overflow-hidden",
        @class ||
          "h-[3.25rem] w-[3.25rem] rounded-xl ring-1 ring-black/[0.08]"
      ]}
      aria-hidden="true"
    >
      <img
        src={@mark_src}
        alt=""
        class="h-full w-full object-contain object-center p-1.5"
        decoding="async"
      />
    </span>
    """
  end

  attr(:entry_id, :string, required: true)
  attr(:label, :string, default: nil)
  attr(:size, :atom, default: :md, values: [:xs, :sm, :md, :lg, :tile])
  attr(:dom_id, :string, default: nil)
  attr(:title, :string, default: nil)
  attr(:img_loading, :string, default: "lazy", values: ["lazy", "eager"])

  def provider_icon(assigns) do
    eid = assigns.entry_id
    spec = simple_icon_spec(eid)
    letters = String.slice(monogram(eid, assigns.label), 0, 2)
    hue = accent_hue(eid)
    dom_id = assigns.dom_id || "mcp-picon-#{:erlang.phash2({eid, assigns.size})}"

    {box, svg_font} =
      case assigns.size do
        :xs -> {"h-6 w-6 rounded-md", 10}
        :sm -> {"h-8 w-8 rounded-md", 12}
        :md -> {"h-10 w-10 rounded-lg", 14}
        :lg -> {"h-12 w-12 rounded-xl", 16}
        :tile -> {"h-[3.25rem] w-[3.25rem] rounded-xl", 17}
      end

    fill = "hsl(#{hue} 48% 38%)"

    assigns =
      assigns
      |> assign(:spec, spec)
      |> assign(:letters, letters)
      |> assign(:hue, hue)
      |> assign(:fill, fill)
      |> assign(:box_class, box)
      |> assign(:svg_font, svg_font)
      |> assign(:dom_id, dom_id)

    ~H"""
    <span
      data-mcp-icon-root
      phx-hook="McpBrandIcon"
      class={["relative inline-flex shrink-0 overflow-hidden ring-1 ring-white/10", @box_class]}
      id={@dom_id}
      title={@title}
    >
      <%= case @spec do %>
        <% {:local, basename} -> %>
          <span
            data-mcp-icon-fallback
            class="absolute inset-0 z-[1] hidden items-center justify-center bg-slate-100/90"
            aria-hidden="true"
          >
            <.monogram_svg letters={@letters} fill={@fill} font_size={@svg_font} />
          </span>
          <img
            src={brand_icon_url(basename)}
            alt=""
            class="brand-icon-img relative z-[2] h-full w-full bg-slate-100/90 object-contain p-1"
            loading={@img_loading}
            decoding="async"
          />
        <% :fallback -> %>
          <span
            class="flex h-full w-full items-center justify-center bg-slate-100/90"
            aria-hidden="true"
          >
            <.monogram_svg letters={@letters} fill={@fill} font_size={@svg_font} />
          </span>
      <% end %>
    </span>
    """
  end

  attr(:letters, :string, required: true)
  attr(:fill, :string, required: true)
  attr(:font_size, :integer, required: true)

  defp monogram_svg(assigns) do
    ~H"""
    <svg
      viewBox="0 0 32 32"
      class="h-full w-full max-h-[90%] max-w-[90%]"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <rect width="32" height="32" rx="8" fill={@fill} />
      <text
        x="16"
        y="16"
        dominant-baseline="central"
        text-anchor="middle"
        fill="#ffffff"
        font-weight="600"
        font-family="ui-sans-serif, system-ui, sans-serif"
        font-size={@font_size}
        letter-spacing="-0.02em"
      >
        {@letters}
      </text>
    </svg>
    """
  end

  attr(:entry_id, :string, required: true)
  attr(:label, :string, default: nil)
  attr(:dom_id, :string, required: true)
  attr(:rest, :global)

  def provider_chip(assigns) do
    assigns =
      assigns
      |> assign(:display, display_label(assigns.entry_id, assigns[:label]))

    ~H"""
    <span
      class={[
        "inline-flex max-w-full items-center gap-2 rounded-full border border-white/10 bg-white/[0.06] py-0.5 pl-1 pr-2.5 text-xs text-white/85 shadow-sm",
        @rest[:class]
      ]}
      title={@entry_id}
    >
      <.provider_icon entry_id={@entry_id} label={@label} size={:xs} dom_id={"#{@dom_id}-icon"} />
      <span class="min-w-0 truncate font-medium">
        {@display}
      </span>
    </span>
    """
  end

  defp display_label(entry_id, label) do
    cond do
      is_binary(label) and String.trim(label) != "" -> label
      true -> entry_id
    end
  end
end
