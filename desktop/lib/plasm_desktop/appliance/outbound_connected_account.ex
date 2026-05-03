defmodule PlasmDesktop.Appliance.OutboundConnectedAccount do
  @moduledoc false
  use Ecto.Schema

  @primary_key {:id, :binary_id, autogenerate: false}
  @foreign_key_type :binary_id

  schema "project_outbound_connected_accounts" do
    belongs_to :auth_config, PlasmDesktop.Appliance.OutboundAuthConfig
    field :owner_subject, :string
    field :external_user_id, :string
    field :hosted_kv_key, :string
    field :status, :string, default: "active"
    field :granted_scopes, {:array, :string}, default: []
    field :last_connected_at, :utc_datetime_usec

    timestamps(type: :utc_datetime_usec)
  end
end
