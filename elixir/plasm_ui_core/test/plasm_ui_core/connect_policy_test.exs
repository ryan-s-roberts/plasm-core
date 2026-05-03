defmodule PlasmUiCore.ConnectPolicyTest do
  use ExUnit.Case, async: true

  alias PlasmUiCore.ConnectPolicy
  alias PlasmUiCore.ConnectProfile

  defp oauth_only, do: profile(:oauth_only)
  defp mixed, do: profile(:api_key_and_oauth)

  defp profile(which) do
    case which do
      :oauth_only ->
        %ConnectProfile{
          capability: :oauth_only,
          oauth: %{provider_present: true, scope_catalog_present: false},
          has_public_mode: false,
          has_api_key: false,
          has_oauth: true
        }

      :api_key_only ->
        %ConnectProfile{
          capability: :api_key_only,
          oauth: %{provider_present: false, scope_catalog_present: false},
          has_public_mode: false,
          has_api_key: true,
          has_oauth: false
        }

      :api_key_and_oauth ->
        %ConnectProfile{
          capability: :api_key_and_oauth,
          oauth: %{provider_present: true, scope_catalog_present: false},
          has_public_mode: false,
          has_api_key: true,
          has_oauth: true
        }
    end
  end

  test "personal hides oauth-only row when Ops OAuth is not ready" do
    p = oauth_only()
    assert ConnectPolicy.hide_personal_catalog_row?(p, false)
    refute ConnectPolicy.hide_personal_catalog_row?(p, true)
  end

  test "missing profile hides personal catalog row" do
    assert ConnectPolicy.hide_personal_catalog_row?(nil, true)
  end

  test "mixed auth degrades to api_key when Ops OAuth missing (personal resolve)" do
    p = mixed()
    assert {:ok, "api_key"} == ConnectPolicy.resolve_personal_connect_kind(p, false)
  end

  test "oauth-only without Ops is an error on connect (personal resolve)" do
    p = oauth_only()
    assert {:error, :oauth_requires_ops} == ConnectPolicy.resolve_personal_connect_kind(p, false)
  end

  test "org create kinds omit oauth2 when Ops app missing" do
    p = mixed()
    assert ["api_key"] == ConnectPolicy.valid_create_auth_kinds(p, false)

    assert Enum.sort(["oauth2", "api_key"]) ==
             Enum.sort(ConnectPolicy.valid_create_auth_kinds(p, true))
  end

  test "migration oauth2 -> api_key when catalog supports api_key" do
    p = mixed()
    assert ConnectPolicy.migration_allowed?(p, false, "oauth2", "api_key")
    refute ConnectPolicy.migration_allowed?(p, false, "oauth2", "none")
  end

  test "migration api_key -> oauth2 when Ops ready and catalog supports oauth" do
    p = mixed()
    assert ConnectPolicy.migration_allowed?(p, true, "api_key", "oauth2")
    refute ConnectPolicy.migration_allowed?(p, false, "api_key", "oauth2")
  end
end
