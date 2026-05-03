defmodule PlasmUiCore.Web.McpRegistryVisualsTest do
  use ExUnit.Case, async: true

  alias PlasmUiCore.Assets
  alias PlasmUiCore.Web.McpRegistryVisuals

  test "simple_icon_spec resolves prefixed entry ids" do
    assert {:local, "clickup"} == McpRegistryVisuals.simple_icon_spec("clickup-b24452")
  end

  test "simple_icon_spec falls back for unknown ids" do
    assert :fallback == McpRegistryVisuals.simple_icon_spec("unknown-api-xyz")
  end

  test "brand_icon_url is under plasm_ui_core static mount" do
    url = McpRegistryVisuals.brand_icon_url("github")
    assert url == Assets.brand_icon_href("github")
    assert url =~ "/vendor/plasm-ui-core/"
    assert url =~ "brand-icons/github.svg"
  end
end
