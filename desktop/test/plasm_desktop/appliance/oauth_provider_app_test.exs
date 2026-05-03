defmodule PlasmDesktop.Appliance.OauthProviderAppTest do
  use PlasmDesktop.DataCase, async: true

  alias PlasmDesktop.Appliance.OauthProviderApp

  test "changeset requires entry_id, client_id, client_secret_key" do
    cs = OauthProviderApp.changeset(%OauthProviderApp{}, %{})
    refute cs.valid?
  end

  test "changeset validates client_secret_key prefix" do
    cs =
      OauthProviderApp.changeset(%OauthProviderApp{}, %{
        "entry_id" => "linear",
        "client_id" => "cid",
        "client_secret_key" => "bad:key"
      })

    refute cs.valid?
  end

  test "secret_key_for_entry is stable" do
    assert OauthProviderApp.secret_key_for_entry("github") ==
             "plasm:oauth_app:v1:github:secret"
  end
end
