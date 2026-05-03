defmodule PlasmUiCore do
  @moduledoc """
  OSS-eligible UI behaviours and shared helpers for Plasm Desktop (plasm-core) and SaaS `web/`.

  **Behaviours:** [`PlasmUiCore.ToolExplorer`](`PlasmUiCore.ToolExplorer`) — agent discovery / tool-model HTTP.

  **Connect policy:** [`PlasmUiCore.ConnectProfile`](`PlasmUiCore.ConnectProfile`),
  [`PlasmUiCore.ConnectPolicy`](`PlasmUiCore.ConnectPolicy`),
  [`PlasmUiCore.ConnectCatalog`](`PlasmUiCore.ConnectCatalog`).

  **Web (SaaS-oriented components):** `PlasmUiCore.Web.*` — shell/modals, OAuth scope editor, copy rows,
  registry brand icons ([`PlasmUiCore.Web.McpRegistryVisuals`](`PlasmUiCore.Web.McpRegistryVisuals`)).

  **Static CSS:** shared tokens and component classes ship under `priv/static/css/plasm_ui_core.css`.
  Host apps mount them with `Plug.Static` at `PlasmUiCore.Assets.static_mount_path/0` and link via
  `PlasmUiCore.Assets.stylesheet_href/1`.
  """
end
