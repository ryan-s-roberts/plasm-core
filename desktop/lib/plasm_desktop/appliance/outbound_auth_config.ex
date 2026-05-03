defmodule PlasmDesktop.Appliance.OutboundAuthConfig do
  @moduledoc false
  use Ecto.Schema

  @primary_key {:id, :binary_id, autogenerate: false}
  @foreign_key_type :binary_id

  schema "project_outbound_auth_configs" do
    field :tenant_id, :string
    field :workspace_slug, :string
    field :project_slug, :string
    field :space_type, :string, default: "organization"
    field :owner_subject, :string
    field :registry_entry_id, :string
    field :auth_kind, :string
    field :name, :string
    field :status, :string, default: "enabled"
    field :oauth_scope_set_name, :string
    field :oauth_scopes, {:array, :string}, default: []

    has_many :connected_accounts, PlasmDesktop.Appliance.OutboundConnectedAccount,
      foreign_key: :auth_config_id

    timestamps(type: :utc_datetime_usec)
  end
end
