defmodule PlasmUiCore.Assets do
  @moduledoc """
  Static asset paths for `plasm_ui_core` when the host application mounts
  `Plug.Static` for this dependency (see `static_mount_path/0`).
  """

  @vendor_prefix "/vendor/plasm-ui-core"

  @doc """
  URL path prefix where the host must mount `Plug.Static` with `from: :plasm_ui_core`.
  Files live under this dependency's `priv/static/` (e.g. `css/plasm_ui_core.css`).
  """
  def static_mount_path, do: @vendor_prefix

  @doc """
  Stylesheet URL for the shared Plasm UI kit (desktop appliance shell + components).

  Appends a cache-busting query from `Application.spec(:plasm_ui_core, :vsn)` unless
  `vsn: false` is passed.
  """
  def stylesheet_href(opts \\ []) do
    path = static_mount_path() <> "/css/plasm_ui_core.css"

    if Keyword.get(opts, :vsn, true) do
      vsn =
        case Application.spec(:plasm_ui_core, :vsn) do
          v when is_list(v) -> IO.chardata_to_string(v)
          v when v != nil -> to_string(v)
          _ -> "dev"
        end

      path <> "?v=" <> URI.encode_www_form(vsn)
    else
      path
    end
  end

  @doc """
  URL path for a vendored registry brand SVG (basename without `.svg`).
  Host must serve `priv/static` from `:plasm_ui_core` (see `static_mount_path/0`).
  """
  def brand_icon_href(basename) when is_binary(basename) do
    static_mount_path() <> "/images/brand-icons/#{basename}.svg"
  end

  @doc "URL path for the small Plasm product mark SVG."
  def plasm_mark_href do
    static_mount_path() <> "/images/plasm-mark.svg"
  end
end
