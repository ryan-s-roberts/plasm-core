defmodule PlasmUiCore.ConnectPolicy do
  @moduledoc """
  Resolves effective outbound-connect behavior from a typed [`PlasmUiCore.ConnectProfile`],
  workspace mode, and Ops OAuth readiness.

  This is the single policy boundary for the personal registry catalog, organization auth-config flows,
  and route guards — callers must not bypass [`PlasmUiCore.ConnectProfile`].
  """

  alias PlasmUiCore.ConnectProfile

  defmodule CatalogConnectState do
    @moduledoc false
    @enforce_keys [
      :profile,
      :workspace,
      :ops_oauth_ready,
      :hidden_from_catalog,
      :display_auth_kinds
    ]
    defstruct [
      :profile,
      :workspace,
      :ops_oauth_ready,
      :hidden_from_catalog,
      :display_auth_kinds,
      oauth_chip_degraded: false
    ]
  end

  @type workspace :: :personal | :organization

  @doc """
  When true, the catalog row must not appear in the personal registry.

  Hidden only when outbound OAuth is the **sole** advertised path (`has_oauth`, no `has_api_key`,
  no `has_public_mode`) and Ops has not registered an OAuth app yet. Mixed catalogs must stay
  visible so users can connect via API key even if `capability` was authored as `oauth_only`.
  """
  def hide_personal_catalog_row?(nil, _), do: true

  def hide_personal_catalog_row?(%ConnectProfile{} = p, ops_oauth_ready)
      when is_boolean(ops_oauth_ready) do
    oauth_only_no_escape_hatch =
      p.has_oauth and not p.has_api_key and not p.has_public_mode

    oauth_only_no_escape_hatch and not ops_oauth_ready
  end

  @doc """
  Kinds to render as chips: mirrors CGS, but in **personal** mode the OAuth chip is suppressed
  when Ops is not ready.
  """
  def display_auth_kinds_for_chips(nil, workspace, ops_oauth_ready)
      when workspace in [:personal, :organization] and is_boolean(ops_oauth_ready) do
    {[], false}
  end

  def display_auth_kinds_for_chips(%ConnectProfile{} = p, workspace, ops_oauth_ready)
      when workspace in [:personal, :organization] and is_boolean(ops_oauth_ready) do
    base = ConnectProfile.to_auth_kinds_list(p)

    case workspace do
      :organization ->
        {base, false}

      :personal ->
        oauth2? = "oauth2" in base
        oauth_ok? = oauth2? and ops_oauth_ready

        kinds =
          if oauth2? and not oauth_ok? do
            Enum.reject(base, &(&1 == "oauth2"))
          else
            base
          end

        {kinds, oauth2? and not oauth_ok?}
    end
  end

  @doc false
  def catalog_connect_state(%ConnectProfile{} = p, workspace, ops_oauth_ready)
      when workspace in [:personal, :organization] and is_boolean(ops_oauth_ready) do
    hidden = workspace == :personal and hide_personal_catalog_row?(p, ops_oauth_ready)
    {display_kinds, degraded?} = display_auth_kinds_for_chips(p, workspace, ops_oauth_ready)

    %CatalogConnectState{
      profile: p,
      workspace: workspace,
      ops_oauth_ready: ops_oauth_ready,
      hidden_from_catalog: hidden,
      display_auth_kinds: display_kinds,
      oauth_chip_degraded: degraded? == true
    }
  end

  @doc """
  Valid auth kinds for **creating** an organization auth config (modal / API).
  OAuth is listed only when the catalog supports OAuth **and** Ops outbound OAuth is ready.
  """
  def valid_create_auth_kinds(nil, _), do: []

  def valid_create_auth_kinds(%ConnectProfile{} = p, ops_oauth_ready)
      when is_boolean(ops_oauth_ready) do
    []
    |> then(fn ks ->
      if p.has_oauth and ops_oauth_ready, do: ks ++ ["oauth2"], else: ks
    end)
    |> then(fn ks -> if p.has_api_key, do: ks ++ ["api_key"], else: ks end)
    |> then(fn ks -> if p.has_public_mode, do: ks ++ ["none"], else: ks end)
    |> Enum.uniq()
  end

  @doc """
  Resolves which stored `auth_kind` to use for a **new** connect attempt (personal flow).

  OAuth wins when Ops is ready; otherwise API key; mixed without Ops degrades to API key;
  OAuth-only without Ops is an error.
  """
  def resolve_personal_connect_kind(nil, _) do
    {:error, :missing_connect_profile}
  end

  def resolve_personal_connect_kind(%ConnectProfile{} = p, ops_oauth_ready)
      when is_boolean(ops_oauth_ready) do
    oauth_ready_effective = p.has_oauth and ops_oauth_ready

    cond do
      oauth_ready_effective ->
        {:ok, "oauth2"}

      p.has_api_key ->
        {:ok, "api_key"}

      p.has_oauth and not ops_oauth_ready ->
        {:error, :oauth_requires_ops}

      true ->
        {:ok, "none"}
    end
  end

  @doc """
  Whether an existing auth config kind may be **migrated** to match the resolved connect kind
  when CGS/Ops drift (mixed-auth catalogs, Ops toggled).
  """
  def migration_allowed?(nil, _, _, _), do: false

  def migration_allowed?(%ConnectProfile{} = p, ops_oauth_ready, from_kind, to_kind)
      when is_boolean(ops_oauth_ready) and is_binary(from_kind) and is_binary(to_kind) do
    case {from_kind, to_kind} do
      {"oauth2", "api_key"} -> p.has_api_key
      {"api_key", "oauth2"} -> p.has_oauth and ops_oauth_ready
      _ -> false
    end
  end
end
