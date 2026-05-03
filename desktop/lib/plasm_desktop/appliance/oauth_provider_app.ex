defmodule PlasmDesktop.Appliance.OauthProviderApp do
  @moduledoc """
  OAuth provider app metadata for outbound link flows (per registry `entry_id`),
  persisted on the appliance and synced to plasm-mcp.
  """
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @foreign_key_type :binary_id

  schema "oauth_provider_apps" do
    field(:entry_id, :string)
    field(:provider, :string)
    field(:authorization_endpoint, :string)
    field(:token_endpoint, :string)
    field(:client_id, :string)
    field(:client_secret_key, :string)
    field(:redirect_uri_note, :string)
    field(:docs_url, :string)
    field(:enabled, :boolean, default: true)
    field(:last_synced_at, :utc_datetime_usec)
    field(:last_sync_error, :string)
    field(:updated_by_subject, :string)

    timestamps(type: :utc_datetime_usec)
  end

  @doc "KV key for client secret in auth-framework storage (must stay within 255 chars)."
  def secret_key_for_entry(entry_id) when is_binary(entry_id) do
    "plasm:oauth_app:v1:#{entry_id}:secret"
  end

  def changeset(struct \\ %__MODULE__{}, attrs) do
    struct
    |> cast(attrs, [
      :entry_id,
      :provider,
      :authorization_endpoint,
      :token_endpoint,
      :client_id,
      :client_secret_key,
      :redirect_uri_note,
      :docs_url,
      :enabled,
      :updated_by_subject
    ])
    |> validate_required([:entry_id, :client_id, :client_secret_key])
    |> validate_length(:entry_id, max: 120)
    |> validate_format(:client_secret_key, ~r/^plasm:(oauth_app:v1:|outbound:)/,
      message: "must start with plasm:oauth_app:v1: or plasm:outbound:"
    )
    |> unique_constraint(:entry_id)
  end
end
