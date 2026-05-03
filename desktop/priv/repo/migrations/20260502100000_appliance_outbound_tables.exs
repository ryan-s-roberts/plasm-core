defmodule PlasmDesktop.Repo.Migrations.ApplianceOutboundTables do
  use Ecto.Migration

  def change do
    create table(:project_outbound_auth_configs, primary_key: false) do
      add :id, :binary_id, primary_key: true
      add :tenant_id, :string, null: false
      add :workspace_slug, :string, null: false
      add :project_slug, :string, null: false
      add :space_type, :string, null: false, default: "organization"
      add :owner_subject, :string
      add :registry_entry_id, :string, null: false
      add :auth_kind, :string, null: false
      add :name, :string, null: false
      add :status, :string, null: false, default: "enabled"
      add :oauth_scope_set_name, :string
      add :oauth_scopes, {:array, :string}, null: false, default: []
      timestamps(type: :utc_datetime_usec)
    end

    create index(:project_outbound_auth_configs, [:tenant_id, :workspace_slug, :project_slug])
    create index(:project_outbound_auth_configs, [:registry_entry_id])

    create table(:project_outbound_connected_accounts, primary_key: false) do
      add :id, :binary_id, primary_key: true

      add :auth_config_id,
          references(:project_outbound_auth_configs, type: :binary_id, on_delete: :delete_all),
          null: false

      add :owner_subject, :string
      add :external_user_id, :string
      add :hosted_kv_key, :string, null: false
      add :status, :string, null: false, default: "active"
      add :granted_scopes, {:array, :string}, null: false, default: []
      add :last_connected_at, :utc_datetime_usec
      add :last_oauth_error, :text
      add :last_oauth_error_at, :utc_datetime_usec
      timestamps(type: :utc_datetime_usec)
    end

    create index(:project_outbound_connected_accounts, [:auth_config_id])
    create unique_index(:project_outbound_connected_accounts, [:hosted_kv_key])
  end
end
