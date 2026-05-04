defmodule PlasmUiCore.Web.Shell do
  @moduledoc """
  Shared document/modal primitives extracted from SaaS `SaasShell` for reuse in `web/` and tooling.
  """
  use Phoenix.Component

  import PlasmUiCore.Web.CoreComponents, only: [icon: 1]

  alias Phoenix.LiveView.JS

  @doc """
  Vertical section stack with consistent spacing between major blocks.
  """
  attr(:id, :string, default: nil)
  attr(:class, :any, default: nil)
  slot(:inner_block, required: true)

  def doc_section(assigns) do
    ~H"""
    <section id={@id} class={["space-y-3", @class]}>
      {render_slot(@inner_block)}
    </section>
    """
  end

  @doc """
  Section title row: heading, optional description, optional actions slot.
  """
  attr(:title, :string, required: true)
  attr(:description, :string, default: nil)
  slot(:actions, required: false)

  def doc_section_header(assigns) do
    ~H"""
    <div class="flex flex-col gap-1 sm:flex-row sm:items-end sm:justify-between">
      <div>
        <h2 class="text-lg font-semibold tracking-tight text-white/92">{@title}</h2>
        <p :if={@description} class="mt-0.5 max-w-prose text-sm text-white/72">
          {@description}
        </p>
      </div>
      <div class="flex shrink-0 flex-wrap gap-2 empty:hidden">
        {render_slot(@actions)}
      </div>
    </div>
    """
  end

  @doc """
  Inset card on the document surface (nested panels, config rows).
  """
  attr(:class, :any, default: nil)
  attr(:padding, :atom, default: :md, values: [:sm, :md, :lg])
  attr(:accent, :atom, default: :none, values: [:none, :plasma])
  attr(:interactive, :boolean, default: false)

  slot(:inner_block, required: true)

  def doc_card(assigns) do
    pad =
      case assigns.padding do
        :sm -> "p-4"
        :lg -> "p-6 sm:p-8"
        :md -> "p-5 sm:p-6"
      end

    accent_class =
      case assigns.accent do
        :plasma -> "plasm-atmo-card"
        _ -> nil
      end

    interactive_class = if assigns.interactive, do: doc_card_interactive_class(), else: nil

    assigns =
      assigns
      |> assign(:pad, pad)
      |> assign(:accent_class, accent_class)
      |> assign(:interactive_class, interactive_class)

    ~H"""
    <div class={[
      "rounded-xl border border-white/[0.1] bg-white/[0.03]",
      @pad,
      @accent_class,
      @interactive_class,
      @class
    ]}>
      {render_slot(@inner_block)}
    </div>
    """
  end

  def doc_card_interactive_class, do: "plasm-surface-interactive"

  @doc """
  Dense shared text/search input for shell toolbars and filters.
  """
  attr(:id, :string, default: nil)
  attr(:name, :string, default: nil)
  attr(:type, :string, default: "text")
  attr(:value, :any, default: nil)
  attr(:placeholder, :string, default: nil)
  attr(:class, :any, default: nil)

  attr(:rest, :global,
    include:
      ~w(accept autocomplete disabled form max maxlength min minlength pattern placeholder readonly required step phx-debounce)
  )

  def control_input(assigns) do
    ~H"""
    <input
      id={@id}
      name={@name}
      type={@type}
      value={@value}
      placeholder={@placeholder}
      class={["plasm-input", @class]}
      {@rest}
    />
    """
  end

  @doc """
  Compact icon-only action button used in dense lists, chips, and modal chrome.
  """
  attr(:icon, :string, required: true)
  attr(:label, :string, required: true)
  attr(:tone, :atom, default: :neutral, values: [:neutral, :danger])
  attr(:size, :atom, default: :sm, values: [:sm, :md])
  attr(:class, :any, default: nil)
  attr(:rest, :global)

  def icon_action_button(assigns) do
    tone_class =
      case assigns.tone do
        :danger -> "text-rose-300/80 hover:bg-rose-400/15 hover:text-rose-200"
        :neutral -> "text-white/52 hover:bg-white/[0.06] hover:text-white/86"
      end

    size_class =
      case assigns.size do
        :md -> "rounded-lg p-2"
        :sm -> "rounded-md p-1"
      end

    icon_size =
      case assigns.size do
        :md -> "size-5"
        :sm -> "size-4"
      end

    assigns =
      assigns
      |> assign(:tone_class, tone_class)
      |> assign(:size_class, size_class)
      |> assign(:icon_size, icon_size)

    ~H"""
    <button
      type="button"
      class={["shrink-0 cursor-pointer transition", @size_class, @tone_class, @class]}
      aria-label={@label}
      {@rest}
    >
      <span class="sr-only">{@label}</span>
      <.icon name={@icon} class={@icon_size} />
    </button>
    """
  end

  attr(:id, :string, required: true)

  attr(:close_event, :string,
    required: true,
    doc: "LiveView event name for backdrop click and Escape."
  )

  attr(:z, :string,
    default: "z-[106]",
    doc: "Tailwind z-index class (raise when nesting modals)."
  )

  attr(:max_w, :string, default: "max-w-5xl", doc: "Max width of the card column.")

  attr(:card_padding, :atom,
    default: :lg,
    values: [:sm, :md, :lg]
  )

  attr(:card_class, :any, default: nil)

  slot(:title_row, required: true)
  slot(:body, required: true)

  slot(:footer,
    required: false,
    doc: "Optional sticky footer below the scrolling body (e.g. primary actions)."
  )

  def stacked_modal(assigns) do
    ~H"""
    <.portal id={"portal-" <> @id} target="body">
      <div
        id={@id}
        class={[
          "fixed inset-0",
          @z,
          "flex min-h-0 flex-col overflow-hidden",
          "pl-[max(1rem,env(safe-area-inset-left,0px))] pr-[max(1rem,env(safe-area-inset-right,0px))] pt-[max(1rem,env(safe-area-inset-top,0px))] pb-[max(1rem,env(safe-area-inset-bottom,0px))]",
          "sm:pl-[max(2rem,env(safe-area-inset-left,0px))] sm:pr-[max(2rem,env(safe-area-inset-right,0px))] sm:pt-[max(2rem,env(safe-area-inset-top,0px))] sm:pb-[max(2rem,env(safe-area-inset-bottom,0px))]"
        ]}
        phx-window-keydown={@close_event}
        phx-key="Escape"
        phx-mounted={
          JS.add_class("overflow-hidden", to: "html") |> JS.add_class("overflow-hidden", to: "body")
        }
        phx-remove={
          JS.remove_class("overflow-hidden", to: "html")
          |> JS.remove_class("overflow-hidden", to: "body")
        }
      >
        <div
          class="absolute inset-0 z-0 bg-slate-950 backdrop-blur-sm"
          phx-click={@close_event}
          aria-hidden="true"
        />
        <div class="relative z-10 flex min-h-0 flex-1 items-start justify-center overflow-hidden">
          <div class={[
            "relative my-4 flex w-full min-h-0 max-h-[min(100dvh-2rem,calc(100vh-2rem))] flex-col sm:my-8",
            @max_w
          ]}>
            <.doc_card
              padding={@card_padding}
              class={[
                "flex min-h-0 max-h-full flex-col overflow-hidden !border-white/25 !bg-slate-950 shadow-[0_40px_110px_-35px_rgba(0,0,0,0.72)]",
                @card_class
              ]}
            >
              <div class="shrink-0">{render_slot(@title_row)}</div>
              <div class="min-h-0 flex-1 overflow-y-auto overscroll-y-contain">
                {render_slot(@body)}
              </div>
              <div :if={@footer != []} class="shrink-0 border-t border-white/[0.06]">
                {render_slot(@footer)}
              </div>
            </.doc_card>
          </div>
        </div>
      </div>
    </.portal>
    """
  end

  attr(:title, :string, required: true)
  attr(:subtitle, :string, default: nil)
  attr(:close_event, :string, required: true)

  def modal_title_row(assigns) do
    ~H"""
    <div class="mb-4 flex items-start justify-between gap-4 border-b border-white/[0.08] pb-4">
      <div>
        <h2 class="text-lg font-semibold tracking-tight text-white/92">{@title}</h2>
        <p :if={@subtitle} class="text-xs text-white/62">{@subtitle}</p>
      </div>
      <button
        type="button"
        phx-click={@close_event}
        class="plasm-modal-chrome-btn rounded-md border border-white/15 bg-white/[0.05] px-2.5 py-1 text-xs font-medium text-white/80 hover:bg-white/[0.1]"
      >
        Close
      </button>
    </div>
    """
  end

  @doc """
  Document column inside the document panel (full-bleed by default).
  """
  attr(:class, :any, default: nil)
  attr(:size, :atom, default: :full, values: [:xl, :lg, :md, :narrow, :full])

  attr(:page_pad, :atom,
    default: :default,
    values: [:default, :roomy, :spacious]
  )

  slot(:inner_block, required: true)

  def doc_page(assigns) do
    max =
      case assigns.size do
        :lg -> "max-w-4xl"
        :md -> "max-w-3xl"
        :narrow -> "max-w-[58rem]"
        :full -> "max-w-none"
        :xl -> "max-w-6xl"
      end

    pad =
      case assigns.page_pad do
        :roomy -> "pb-12"
        :spacious -> "pb-10"
        _ -> "pb-8"
      end

    body =
      case assigns.size do
        :full -> ["w-full max-w-none space-y-4", pad, assigns.class]
        _ -> ["mx-auto w-full space-y-4", pad, max, assigns.class]
      end

    assigns = assign(assigns, :body_class, body)

    ~H"""
    <div class={@body_class}>
      {render_slot(@inner_block)}
    </div>
    """
  end

  @doc """
  Local readability wrapper for long prose blocks — use inside tables/cards.
  """
  attr(:class, :any, default: nil)
  slot(:inner_block, required: true)

  def doc_prose(assigns) do
    ~H"""
    <div class={["max-w-prose text-sm leading-relaxed text-white/72", @class]}>
      {render_slot(@inner_block)}
    </div>
    """
  end

  @doc """
  Base class for full-width interactive catalog rows (`<.link>`, `<button>`).
  """
  def catalog_row_class, do: "plasm-row-interactive"

  @doc """
  Canonical surface for list/table containers (tool catalog, explorer chrome).
  """
  attr(:class, :any, default: nil)
  attr(:overflow_hidden, :boolean, default: true)
  slot(:inner_block, required: true)

  def table_surface(assigns) do
    ~H"""
    <section class={[
      "rounded-md border border-white/[0.1] bg-white/[0.02] p-0",
      @overflow_hidden && "overflow-hidden",
      @class
    ]}>
      {render_slot(@inner_block)}
    </section>
    """
  end

  @doc """
  Dashed empty / placeholder surface for lists and tables.
  """
  attr(:title, :string, required: true)
  attr(:description, :string, default: nil)
  attr(:tone, :atom, default: :neutral, values: [:neutral, :warning])
  attr(:class, :any, default: nil)

  slot(:inner_block, required: false)

  def doc_empty_state(assigns) do
    surface =
      case assigns.tone do
        :warning ->
          "rounded-xl border border-dashed border-amber-300/35 bg-amber-300/12 px-8 py-14 text-center"

        :neutral ->
          "rounded-xl border border-dashed border-white/20 bg-white/[0.025] px-8 py-14 text-center"
      end

    assigns = assign(assigns, :surface, surface)

    ~H"""
    <div class={[@surface, @class]}>
      <p class="text-sm font-medium text-white/78">{@title}</p>
      <p :if={@description} class="mx-auto mt-2 max-w-md text-sm text-white/56">
        {@description}
      </p>
      {render_slot(@inner_block)}
    </div>
    """
  end
end
