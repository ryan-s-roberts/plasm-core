defmodule PlasmUiCore.Web.CoreComponents do
  @moduledoc """
  Minimal Phoenix components shared by SaaS `web/` and extracted shells (buttons, icons).
  """
  use Phoenix.Component

  @doc """
  Renders a Heroicon span (requires host CSS / Tailwind plugin for `hero-*` classes).
  """
  attr(:name, :string, required: true)
  attr(:class, :any, default: "size-4")

  def icon(%{name: "hero-" <> _} = assigns) do
    ~H"""
    <span class={[@name, @class]} />
    """
  end

  @doc """
  Primary action button (matches SaaS `plasm-btn` styling).
  """
  attr(
    :rest,
    :global,
    include: ~w(
        id class title role aria-label aria-hidden aria-expanded aria-describedby
        href navigate patch method download name value disabled type
        phx-click phx-submit phx-change phx-disable-with phx-target phx-hook phx-mounted
        phx-window-keydown phx-key
        phx-value-id phx-value-step phx-value-entry_id phx-value-kind phx-value-set phx-value-scope
        data-plasm-interstitial-busy
      )
  )

  attr(:class, :any)

  attr(:variant, :string,
    values: ~w(primary secondary quiet danger),
    doc: "`primary` | `secondary` | `quiet` | `danger`"
  )

  attr(:size, :string,
    default: "md",
    values: ~w(xs sm md),
    doc: "`xs` | `sm` | `md`"
  )

  slot(:inner_block, required: true)

  def button(%{rest: rest} = assigns) do
    variant = assigns[:variant] || "primary"
    size = assigns[:size] || "md"

    variant_class =
      case variant do
        "primary" -> "plasm-btn plasm-btn-primary"
        "secondary" -> "plasm-btn plasm-btn-secondary"
        "quiet" -> "plasm-btn plasm-btn-quiet"
        "danger" -> "plasm-btn plasm-btn-danger"
        _ -> "plasm-btn plasm-btn-primary"
      end

    size_class =
      case size do
        "xs" -> "min-h-8 px-2.5 py-1 text-xs"
        "sm" -> "min-h-8.5 px-3 py-1.5 text-xs"
        _ -> "min-h-9 px-3.5 py-2 text-[13px]"
      end

    assigns =
      assign(assigns, :class, [variant_class, size_class, assigns[:class]])

    if rest[:href] || rest[:navigate] || rest[:patch] do
      ~H"""
      <.link class={@class} {@rest}>
        {render_slot(@inner_block)}
      </.link>
      """
    else
      ~H"""
      <button class={@class} {@rest}>
        {render_slot(@inner_block)}
      </button>
      """
    end
  end
end
