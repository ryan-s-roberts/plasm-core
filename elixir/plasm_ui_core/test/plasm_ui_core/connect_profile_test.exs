defmodule PlasmUiCore.ConnectProfileTest do
  use ExUnit.Case, async: true

  alias PlasmUiCore.ConnectProfile

  test "decodes typed connect_profile JSON" do
    auth = %{
      "connect_profile" => %{
        "capability" => "oauth_only",
        "oauth" => %{"provider_present" => true, "scope_catalog_present" => true},
        "has_public_mode" => false,
        "has_api_key" => false,
        "has_oauth" => true
      }
    }

    assert {:ok, p} = ConnectProfile.from_auth(auth)
    assert p.capability == :oauth_only
    assert p.has_oauth
    assert p.oauth.provider_present
  end

  test "rejects auth without connect_profile" do
    assert :error == ConnectProfile.from_auth(%{})
    assert :error == ConnectProfile.from_auth(%{"supported_auth_kinds" => ["oauth2"]})
  end
end
