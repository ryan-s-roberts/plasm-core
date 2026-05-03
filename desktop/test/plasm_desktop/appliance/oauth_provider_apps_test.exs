defmodule PlasmDesktop.Appliance.OauthProviderAppsTest do
  use PlasmDesktop.DataCase, async: true

  alias PlasmDesktop.Appliance.OauthProviderApps

  test "upsert_oauth_provider_app rejects raw client_secret in attrs" do
    assert {:error, cs} =
             OauthProviderApps.upsert_oauth_provider_app(%{
               "entry_id" => "x",
               "client_id" => "c",
               "client_secret_key" => "plasm:oauth_app:v1:x:secret",
               "client_secret" => "nope"
             })

    refute cs.valid?
  end

  test "upsert_oauth_provider_app persists metadata" do
    assert {:ok, app} =
             OauthProviderApps.upsert_oauth_provider_app(%{
               "entry_id" => "demo_api",
               "client_id" => "cid",
               "authorization_endpoint" => "https://idp.example/oauth/authorize",
               "token_endpoint" => "https://idp.example/oauth/token",
               "enabled" => true
             })

    assert app.entry_id == "demo_api"
    assert app.client_secret_key == "plasm:oauth_app:v1:demo_api:secret"

    assert %PlasmDesktop.Appliance.OauthProviderApp{} =
             OauthProviderApps.get_oauth_provider_app_by_entry("demo_api")
  end
end
