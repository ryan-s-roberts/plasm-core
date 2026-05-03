defmodule PlasmDesktop.Repo.Migrations.CreateOauthProviderApps do
  use Ecto.Migration

  def change do
    create table(:oauth_provider_apps, primary_key: false) do
      add :id, :binary_id, primary_key: true
      add :entry_id, :string, null: false
      add :provider, :string
      add :authorization_endpoint, :text
      add :token_endpoint, :text
      add :client_id, :text, null: false
      add :client_secret_key, :string, null: false
      add :redirect_uri_note, :text
      add :docs_url, :text
      add :enabled, :boolean, null: false, default: true
      add :last_synced_at, :utc_datetime_usec
      add :last_sync_error, :text
      add :updated_by_subject, :string
      timestamps(type: :utc_datetime_usec)
    end

    create unique_index(:oauth_provider_apps, [:entry_id])
  end
end
