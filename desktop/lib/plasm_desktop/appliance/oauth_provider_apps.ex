defmodule PlasmDesktop.Appliance.OauthProviderApps do
  @moduledoc """
  Persistence and agent sync for outbound OAuth provider apps on the appliance.

  Mirrors SaaS `PlasmWeb.Platform` behavior: metadata in Postgres,
  client secrets only via agent KV (`outbound-secrets/put`) + `oauth-link/provider-upsert`.
  """
  import Ecto.Query

  require Logger

  alias PlasmDesktop.Appliance.OauthProviderApp
  alias PlasmDesktop.Mcp.ControlPlane
  alias PlasmDesktop.Repo

  def list_oauth_provider_apps do
    Repo.all(from(o in OauthProviderApp, order_by: o.entry_id))
  end

  def get_oauth_provider_app_by_entry(entry_id) when is_binary(entry_id) do
    entry_id = String.trim(entry_id)
    if entry_id == "", do: nil, else: Repo.get_by(OauthProviderApp, entry_id: entry_id)
  end

  def change_oauth_provider_app(%OauthProviderApp{} = app, attrs \\ %{}) do
    OauthProviderApp.changeset(app, attrs)
  end

  @doc """
  Persist operator edits. Does not call the agent — use `sync_oauth_provider_app_to_agent/3` after.
  """
  def upsert_oauth_provider_app(attrs, subject \\ nil) when is_map(attrs) do
    with :ok <- ensure_metadata_only_attrs(attrs) do
      entry_id = attrs["entry_id"] || attrs[:entry_id]
      entry_id = entry_id |> to_string() |> String.trim()

      if entry_id == "" do
        cs =
          %OauthProviderApp{}
          |> OauthProviderApp.changeset(%{})
          |> Ecto.Changeset.add_error(:entry_id, "can't be blank")

        {:error, cs}
      else
        base_attrs =
          attrs
          |> Map.new(fn {k, v} -> {to_string(k), v} end)
          |> Map.put("entry_id", entry_id)
          |> maybe_put_secret_key(entry_id)
          |> maybe_put_subject(subject)

        existing = get_oauth_provider_app_by_entry(entry_id)

        case existing do
          nil ->
            %OauthProviderApp{}
            |> OauthProviderApp.changeset(base_attrs)
            |> Repo.insert()

          app ->
            app
            |> OauthProviderApp.changeset(base_attrs)
            |> Repo.update()
        end
      end
    end
  end

  defp ensure_metadata_only_attrs(attrs) when is_map(attrs) do
    lowered =
      attrs
      |> Map.keys()
      |> Enum.map(&to_string/1)
      |> Enum.map(&String.downcase/1)
      |> MapSet.new()

    forbidden =
      MapSet.new([
        "client_secret",
        "secret",
        "api_key",
        "access_token",
        "refresh_token",
        "bearer_token"
      ])

    if MapSet.disjoint?(lowered, forbidden) do
      :ok
    else
      cs =
        %OauthProviderApp{}
        |> OauthProviderApp.changeset(%{})
        |> Ecto.Changeset.add_error(
          :base,
          "raw secret material is not persisted here; store via plasm-mcp outbound-secrets API"
        )

      {:error, cs}
    end
  end

  defp maybe_put_secret_key(attrs, entry_id) do
    sk = attrs["client_secret_key"] || attrs[:client_secret_key]

    cond do
      is_binary(sk) and String.trim(sk) != "" ->
        attrs

      true ->
        Map.put(attrs, "client_secret_key", OauthProviderApp.secret_key_for_entry(entry_id))
    end
  end

  defp maybe_put_subject(attrs, nil), do: attrs

  defp maybe_put_subject(attrs, sub) when is_binary(sub) do
    Map.put(attrs, "updated_by_subject", sub)
  end

  @doc """
  Writes optional new `client_secret` to agent KV, then `provider-upsert`.

  Pass `client_secret` only when rotating or first-time set (non-empty string).
  """
  def sync_oauth_provider_app_to_agent(session, %OauthProviderApp{} = app, client_secret \\ nil)
      when is_map(session) do
    with :ok <- maybe_put_client_secret(session, app.client_secret_key, client_secret),
         :ok <- ControlPlane.oauth_link_provider_upsert(session, agent_payload(app)) do
      now = DateTime.utc_now() |> DateTime.truncate(:microsecond)

      Logger.info(
        "[plasm_desktop] oauth_provider_app synced to agent entry_id=#{app.entry_id} enabled=#{app.enabled}"
      )

      app
      |> Ecto.Changeset.change(%{last_synced_at: now, last_sync_error: nil})
      |> Repo.update()
    else
      {:error, reason} ->
        err = inspect(reason, limit: 200)

        Logger.warning(
          "[plasm_desktop] oauth_provider_app sync to agent failed entry_id=#{app.entry_id} reason=#{err}"
        )

        case app
             |> Ecto.Changeset.change(%{last_sync_error: err})
             |> Repo.update() do
          {:ok, _} -> {:error, reason}
          {:error, cs} -> {:error, cs}
        end
    end
  end

  defp maybe_put_client_secret(_session, _key, nil), do: :ok
  defp maybe_put_client_secret(_session, _key, ""), do: :ok

  defp maybe_put_client_secret(session, key, secret) when is_binary(secret) do
    case String.trim(secret) do
      "" -> :ok
      s -> ControlPlane.outbound_secret_put(session, key, s)
    end
  end

  defp agent_payload(%OauthProviderApp{enabled: false} = app) do
    %{
      "entry_id" => app.entry_id,
      "authorization_endpoint" => "",
      "token_endpoint" => "",
      "default_scopes" => [],
      "client_id" => "",
      "client_secret_key" => app.client_secret_key,
      "enabled" => false
    }
  end

  defp agent_payload(%OauthProviderApp{} = app) do
    %{
      "entry_id" => app.entry_id,
      "authorization_endpoint" => app.authorization_endpoint || "",
      "token_endpoint" => app.token_endpoint || "",
      "default_scopes" => [],
      "client_id" => app.client_id,
      "client_secret_key" => app.client_secret_key,
      "enabled" => true
    }
  end
end
