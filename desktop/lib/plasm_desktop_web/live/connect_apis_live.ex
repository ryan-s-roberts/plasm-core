defmodule PlasmDesktopWeb.ConnectApisLive do
  @moduledoc """
  Primary **Connect APIs** surface: catalog picker, guided connect (public / API key / OAuth),
  and MCP transport keys — without exposing control-plane plumbing. Session traces are on `/traces`.
  """
  use PlasmDesktopWeb, :live_view

  import PlasmDesktopWeb.McpKeyUi, only: [row_meta_line: 1, row_primary_title: 1]

  alias PlasmDesktop.Appliance.{CatalogMeta, DefaultMcpAccessKey, YourMcpConnect, YourMcpSnapshot}
  alias PlasmDesktop.Mcp.{ControlPlane, DataPlane}
  alias PlasmUiCore.{ConnectCatalog, ConnectPolicy, ConnectProfile}

  @impl true
  def mount(_params, session, socket) do
    snap = YourMcpSnapshot.build(session)
    gen = 1
    ids = catalog_meta_ids(snap.registry_entries)

    if ids != [] do
      send(self(), {:catalog_meta_fetch, gen, session, ids})
      send(self(), {:oauth_ready_scan, gen, session, snap.registry_entries})
    end

    socket =
      socket
      |> assign(:page_title, "Connect APIs")
      |> assign(:desk_session, session)
      |> assign(:snap, snap)
      |> assign(:catalog_modal, false)
      |> assign(:catalog_query, "")
      |> assign(:catalog_meta, %{})
      |> assign(:catalog_meta_pending, MapSet.new(ids))
      |> assign(:catalog_meta_loading?, ids != [])
      |> assign(:oauth_ready_ids, MapSet.new())
      |> assign(:meta_gen, gen)
      |> assign(:api_key_modal, nil)
      |> assign(:oauth_scope_modal, nil)
      |> assign(:add_key_modal, false)
      |> assign(:last_provisioned_key, nil)

    {:ok, apply_default_access_key(socket)}
  end

  @impl true
  def handle_params(params, _uri, socket) do
    socket =
      case params["oauth_status"] do
        "ok" ->
          entry_id = params["entry_id"] || ""
          kv = params["hosted_kv_key"] || ""

          socket =
            case YourMcpConnect.complete_oauth_return(
                   socket.assigns.desk_session,
                   entry_id,
                   kv,
                   []
                 ) do
              :ok ->
                socket
                |> refresh_snap()
                |> put_flash(:info, "Connected via OAuth.")

              {:error, reason} ->
                put_flash(socket, :error, oauth_complete_message(reason))
            end

          push_patch(socket, to: "/connect-apis", replace: true)

        "error" ->
          msg = params["oauth_error"] || "OAuth failed."

          socket
          |> put_flash(:error, msg)
          |> push_patch(to: "/connect-apis", replace: true)

        _ ->
          socket
      end

    {:noreply, socket}
  end

  @impl true
  def handle_info({:catalog_meta_fetch, gen, sess, ids}, socket) do
    if gen != socket.assigns.meta_gen do
      {:noreply, socket}
    else
      meta =
        ids
        |> Task.async_stream(
          fn eid ->
            case DataPlane.fetch_tool_model(sess, eid, focus: "all") do
              {:ok, body} -> {eid, CatalogMeta.from_tool_model(body)}
              _ -> {eid, nil}
            end
          end,
          max_concurrency: 4,
          timeout: 120_000
        )
        |> Enum.reduce(socket.assigns.catalog_meta, fn
          {:ok, {eid, m}}, acc when is_map(m) -> Map.put(acc, eid, m)
          _, acc -> acc
        end)

      {:noreply,
       socket
       |> assign(:catalog_meta, meta)
       |> assign(:catalog_meta_pending, MapSet.new())
       |> assign(:catalog_meta_loading?, false)}
    end
  end

  def handle_info({:oauth_ready_scan, gen, sess, entries}, socket) do
    if gen != socket.assigns.meta_gen do
      {:noreply, socket}
    else
      ready =
        entries
        |> Task.async_stream(
          fn e ->
            eid = entry_id(e)

            if eid == "" do
              nil
            else
              case DataPlane.fetch_tool_model(sess, eid, focus: "all") do
                {:ok, body} ->
                  case ConnectProfile.from_auth(Map.get(body, "auth") || %{}) do
                    {:ok, p} ->
                      if p.has_oauth and p.oauth.provider_present, do: eid, else: nil

                    _ ->
                      nil
                  end

                _ ->
                  nil
              end
            end
          end,
          max_concurrency: 6,
          timeout: 60_000
        )
        |> Enum.reduce(MapSet.new(), fn
          {:ok, eid}, acc when is_binary(eid) and eid != "" -> MapSet.put(acc, eid)
          _, acc -> acc
        end)

      {:noreply, assign(socket, :oauth_ready_ids, ready)}
    end
  end

  @impl true
  def handle_event("open_catalog_modal", _, socket) do
    {:noreply, assign(socket, :catalog_modal, true)}
  end

  def handle_event("close_catalog_modal", _, socket) do
    {:noreply, assign(socket, :catalog_modal, false)}
  end

  def handle_event("catalog_search", params, socket) do
    q =
      case params do
        %{"q" => v} when is_binary(v) -> v
        %{"q" => v} -> to_string(v)
        _ -> ""
      end

    {:noreply, assign(socket, :catalog_query, q)}
  end

  def handle_event("open_api_key_modal", %{"entry_id" => eid}, socket) do
    eid = String.trim(to_string(eid))
    label = registry_label(socket.assigns.snap.registry_entries, eid)
    {:noreply, assign(socket, :api_key_modal, %{entry_id: eid, label: label})}
  end

  def handle_event("close_api_key_modal", _, socket) do
    {:noreply, assign(socket, :api_key_modal, nil)}
  end

  def handle_event("submit_api_key_connect", params, socket) do
    session = socket.assigns.desk_session

    secret =
      case params["secret"] do
        v when is_binary(v) -> v
        v -> to_string(v || "")
      end

    case socket.assigns.api_key_modal do
      %{entry_id: eid} ->
        cond do
          not socket.assigns.snap.policy.control_plane_ok ->
            {:noreply, put_flash(socket, :error, "Control plane secret is not available.")}

          true ->
            case YourMcpConnect.connect_api_key(session, eid, secret) do
              :ok ->
                {:noreply,
                 socket
                 |> refresh_snap()
                 |> assign(:api_key_modal, nil)
                 |> assign(:catalog_modal, false)
                 |> put_flash(:info, "Connected with API key.")}

              {:error, {:http_status, status, _}} ->
                {:noreply,
                 put_flash(socket, :error, "Could not connect (#{status}). Check agent logs.")}

              {:error, other} ->
                {:noreply,
                 put_flash(socket, :error, "Could not connect: #{inspect(other, limit: 120)}")}
            end
        end

      _ ->
        {:noreply, socket}
    end
  end

  def handle_event("catalog_connect", %{"entry_id" => raw_eid}, socket) do
    session = socket.assigns.desk_session
    eid = String.trim(to_string(raw_eid))

    meta = Map.get(socket.assigns.catalog_meta, eid, %{})
    profile = ConnectCatalog.connect_profile_from_meta(meta)
    ops = MapSet.member?(socket.assigns.oauth_ready_ids, eid)

    cond do
      not socket.assigns.snap.policy.control_plane_ok ->
        {:noreply, put_flash(socket, :error, "Control plane secret is not available.")}

      eid == "" ->
        {:noreply, put_flash(socket, :error, "Missing catalog id.")}

      true ->
        case ConnectPolicy.resolve_personal_connect_kind(profile, ops) do
          {:ok, "none"} ->
            case YourMcpConnect.connect_public(session, eid) do
              :ok ->
                {:noreply,
                 socket
                 |> refresh_snap()
                 |> assign(:catalog_modal, false)
                 |> put_flash(:info, "App enabled (no credentials required).")}

              {:error, {:http_status, status, _}} ->
                {:noreply,
                 put_flash(socket, :error, "Connect failed (#{status}). Check agent and plugins.")}

              {:error, other} ->
                {:noreply,
                 put_flash(socket, :error, "Connect failed: #{inspect(other, limit: 120)}")}
            end

          {:ok, "api_key"} ->
            handle_event("open_api_key_modal", %{"entry_id" => eid}, socket)

          {:ok, "oauth2"} ->
            oauth_profile = Map.get(meta, "oauth_profile") || %{}
            entries_map = oauth_profile["scope_entries"]

            if is_map(entries_map) and map_size(entries_map) > 0 do
              label = registry_label(socket.assigns.snap.registry_entries, eid)

              modal = %{
                entry_id: eid,
                label: label,
                oauth_profile: oauth_profile,
                scope_draft: default_scope_draft(oauth_profile),
                scope_query: ""
              }

              {:noreply, assign(socket, :oauth_scope_modal, modal)}
            else
              start_oauth_redirect(socket, eid, [])
            end

          {:error, :oauth_requires_ops} ->
            {:noreply,
             put_flash(
               socket,
               :error,
               "OAuth is not available for this catalog yet (provider not configured on the agent)."
             )}

          {:error, :missing_connect_profile} ->
            {:noreply,
             put_flash(
               socket,
               :error,
               "This catalog did not publish connect metadata — retry after plugins load."
             )}
        end
    end
  end

  def handle_event("close_oauth_scope_modal", _, socket) do
    {:noreply, assign(socket, :oauth_scope_modal, nil)}
  end

  def handle_event("appliance_scope_query", params, socket) do
    q = params["q"] || ""

    case socket.assigns.oauth_scope_modal do
      %{scope_query: _} = modal ->
        {:noreply, assign(socket, :oauth_scope_modal, %{modal | scope_query: to_string(q)})}

      _ ->
        {:noreply, socket}
    end
  end

  def handle_event("appliance_scope_add", %{"scope" => scope}, socket) do
    scope = to_string(scope)

    case socket.assigns.oauth_scope_modal do
      %{scope_draft: draft} = modal ->
        draft = if scope in draft, do: draft, else: draft ++ [scope]
        {:noreply, assign(socket, :oauth_scope_modal, %{modal | scope_draft: draft})}

      _ ->
        {:noreply, socket}
    end
  end

  def handle_event("appliance_scope_remove", %{"scope" => scope}, socket) do
    scope = to_string(scope)

    case socket.assigns.oauth_scope_modal do
      %{scope_draft: draft} = modal ->
        {:noreply,
         assign(socket, :oauth_scope_modal, %{
           modal
           | scope_draft: Enum.reject(draft, &(&1 == scope))
         })}

      _ ->
        {:noreply, socket}
    end
  end

  def handle_event("appliance_scope_apply_set", %{"set" => name}, socket) do
    name = to_string(name)

    case socket.assigns.oauth_scope_modal do
      %{oauth_profile: profile} = modal ->
        sets = profile["default_scope_sets"] || %{}

        draft =
          case sets do
            %{^name => list} when is_list(list) -> Enum.map(list, &to_string/1)
            _ -> modal.scope_draft
          end

        {:noreply, assign(socket, :oauth_scope_modal, %{modal | scope_draft: draft})}

      _ ->
        {:noreply, socket}
    end
  end

  def handle_event("appliance_oauth_continue", _, socket) do
    case socket.assigns.oauth_scope_modal do
      %{entry_id: eid, scope_draft: draft} ->
        socket = assign(socket, :oauth_scope_modal, nil)
        start_oauth_redirect(socket, eid, draft)

      _ ->
        {:noreply, socket}
    end
  end

  def handle_event("revoke_connected", %{"entry_id" => raw}, socket) do
    session = socket.assigns.desk_session
    eid = String.trim(to_string(raw))

    cond do
      not socket.assigns.snap.policy.control_plane_ok ->
        {:noreply, put_flash(socket, :error, "Control plane secret is not available.")}

      eid == "" ->
        {:noreply, put_flash(socket, :error, "Missing catalog id.")}

      true ->
        case YourMcpConnect.revoke(session, eid) do
          :ok ->
            {:noreply,
             socket
             |> refresh_snap()
             |> put_flash(
               :info,
               "Disconnected #{registry_label(socket.assigns.snap.registry_entries, eid)}."
             )}

          {:error, {:http_status, status, _}} ->
            {:noreply, put_flash(socket, :error, "Revoke failed (#{status}).")}

          {:error, other} ->
            {:noreply, put_flash(socket, :error, "Revoke failed: #{inspect(other, limit: 120)}")}
        end
    end
  end

  def handle_event("open_add_key_modal", _, socket) do
    {:noreply, assign(socket, :add_key_modal, true)}
  end

  def handle_event("close_add_key_modal", _, socket) do
    {:noreply, assign(socket, :add_key_modal, false)}
  end

  def handle_event("copy_last_provisioned_key", _, socket) do
    case socket.assigns.last_provisioned_key do
      k when is_binary(k) and k != "" ->
        {:noreply,
         socket
         |> push_event("mcp:copy", %{text: k})
         |> put_flash(:info, "Bearer token copied to clipboard.")}

      _ ->
        {:noreply, socket}
    end
  end

  def handle_event("copy_mcp_access_key", %{"key_id" => kid}, socket) do
    session = socket.assigns.desk_session
    snap = socket.assigns.snap
    cid = snap.policy.config_id
    kid = to_string(kid)

    cond do
      not snap.policy.control_plane_ok ->
        {:noreply, put_flash(socket, :error, "Control plane secret is not available.")}

      not is_binary(cid) ->
        {:noreply, put_flash(socket, :error, "No MCP configuration id.")}

      kid == "" ->
        {:noreply, socket}

      true ->
        case ControlPlane.reveal_api_key(session, cid, kid) do
          {:ok, %{"api_key" => key}} when is_binary(key) and key != "" ->
            {:noreply,
             socket
             |> push_event("mcp:copy", %{text: key})
             |> put_flash(:info, "Bearer token copied to clipboard.")}

          {:ok, body} when is_map(body) ->
            key = Map.get(body, "api_key") || Map.get(body, "token")

            if is_binary(key) and key != "" do
              {:noreply,
               socket
               |> push_event("mcp:copy", %{text: key})
               |> put_flash(:info, "Bearer token copied to clipboard.")}
            else
              {:noreply, put_flash(socket, :error, "Could not read API key from agent.")}
            end

          {:error, other} ->
            {:noreply,
             put_flash(socket, :error, "Could not read API key: #{inspect(other, limit: 160)}")}
        end
    end
  end

  def handle_event("add_mcp_access_key", params, socket) do
    session = socket.assigns.desk_session
    snap = socket.assigns.snap

    lab =
      case params["label"] do
        v when is_binary(v) -> String.trim(v)
        _ -> ""
      end

    lab = if lab == "", do: "Unnamed", else: lab
    cid = snap.policy.config_id

    cond do
      not snap.policy.control_plane_ok ->
        {:noreply, put_flash(socket, :error, "Control plane secret is not available.")}

      not is_binary(cid) or not PlasmDesktop.Appliance.McpPayload.valid_uuid?(cid) ->
        {:noreply,
         put_flash(
           socket,
           :error,
           "MCP configuration is not ready yet — check the agent / control plane secret, or retry after the page loads."
         )}

      true ->
        case ControlPlane.provision_api_key(session, %{"config_id" => cid, "label" => lab}) do
          {:ok, body} when is_map(body) ->
            key = Map.get(body, "api_key")

            {:noreply,
             socket
             |> assign(:add_key_modal, false)
             |> assign(:last_provisioned_key, key)
             |> refresh_snap()
             |> put_flash(:info, "New access key — use Copy bearer on the banner or in the list.")}

          {:error, other} ->
            {:noreply,
             put_flash(socket, :error, "Provision failed: #{inspect(other, limit: 160)}")}
        end
    end
  end

  def handle_event("revoke_access_key", %{"key_id" => kid}, socket) do
    session = socket.assigns.desk_session
    snap = socket.assigns.snap
    cid = snap.policy.config_id

    cond do
      not snap.policy.control_plane_ok ->
        {:noreply, put_flash(socket, :error, "Control plane secret is not available.")}

      not is_binary(cid) ->
        {:noreply, put_flash(socket, :error, "No MCP configuration id.")}

      true ->
        case ControlPlane.revoke_api_key(session, cid, to_string(kid)) do
          :ok ->
            {:noreply,
             socket
             |> assign(:last_provisioned_key, nil)
             |> refresh_snap()
             |> put_flash(:info, "Access key revoked.")}

          {:error, other} ->
            {:noreply, put_flash(socket, :error, "Revoke failed: #{inspect(other, limit: 160)}")}
        end
    end
  end

  def handle_event("dismiss_key", _, socket) do
    {:noreply, assign(socket, :last_provisioned_key, nil)}
  end

  defp start_oauth_redirect(socket, eid, scopes) when is_binary(eid) do
    session = socket.assigns.desk_session

    base =
      Application.get_env(:plasm_desktop, :public_desktop_base_url) || "http://127.0.0.1:4000"

    return_url = String.trim_trailing(base, "/") <> "/connect-apis"

    opts = if scopes == [], do: [], else: [scopes: scopes]

    case ControlPlane.oauth_link_start(session, eid, return_url, opts) do
      {:ok, body} when is_map(body) ->
        url = oauth_authorize_url(body)

        if url != "" do
          {:noreply, redirect(socket, external: url)}
        else
          {:noreply,
           put_flash(
             socket,
             :error,
             "OAuth did not return authorize_url — #{inspect(Map.keys(body))}"
           )}
        end

      {:error, reason} ->
        {:noreply,
         put_flash(socket, :error, "OAuth could not start: #{inspect(reason, limit: 160)}")}
    end
  end

  defp oauth_authorize_url(body) when is_map(body) do
    case Map.get(body, "authorize_url") do
      u when is_binary(u) and u != "" -> u
      _ -> Map.get(body, "url") || ""
    end
  end

  defp refresh_snap(socket) do
    session = socket.assigns.desk_session
    snap = YourMcpSnapshot.build(session)
    gen = socket.assigns.meta_gen + 1
    ids = catalog_meta_ids(snap.registry_entries)

    socket =
      socket
      |> assign(:snap, snap)
      |> assign(:meta_gen, gen)

    socket =
      if ids != [] do
        send(self(), {:catalog_meta_fetch, gen, session, ids})
        send(self(), {:oauth_ready_scan, gen, session, snap.registry_entries})

        socket
        |> assign(:catalog_meta_pending, MapSet.new(ids))
        |> assign(:catalog_meta_loading?, true)
      else
        socket
        |> assign(:catalog_meta_pending, MapSet.new())
        |> assign(:catalog_meta_loading?, false)
      end

    apply_default_access_key(socket)
  end

  defp apply_default_access_key(socket) do
    session = socket.assigns.desk_session

    case DefaultMcpAccessKey.bootstrap(session) do
      {:ok, key, snap} ->
        socket
        |> assign(:snap, snap)
        |> assign(:last_provisioned_key, key)
        |> put_flash(
          :info,
          ~s(Default MCP access key "desktop" was created — use Copy bearer on the banner or in the list.)
        )

      {:noop, snap} ->
        assign(socket, :snap, snap)

      {:error, reason, snap} ->
        require Logger

        Logger.warning("[plasm_desktop] default MCP access key: #{inspect(reason, limit: 120)}")

        assign(socket, :snap, snap)
    end
  end

  defp oauth_complete_message(:bad_kv_key),
    do: "OAuth finished but the vault key from the agent was invalid — try again."

  defp oauth_complete_message(:bad_entry),
    do: "OAuth callback missing catalog id."

  defp oauth_complete_message(other),
    do: "Could not finalize OAuth: #{inspect(other, limit: 120)}"

  defp catalog_meta_ids(entries) when is_list(entries) do
    entries
    |> Enum.map(&entry_id/1)
    |> Enum.reject(&(&1 == ""))
    |> Enum.uniq()
    |> Enum.take(64)
  end

  defp default_scope_draft(%{"default_scope_sets" => sets})
       when is_map(sets) and map_size(sets) > 0 do
    sets
    |> Enum.sort_by(fn {k, _} -> k end)
    |> List.first()
    |> elem(1)
    |> List.wrap()
    |> Enum.map(&to_string/1)
  end

  defp default_scope_draft(_), do: []

  defp connected_rows(detail, registry_entries) do
    {bindings, optional, allowed} = YourMcpConnect.base_maps(detail)
    optional_set = MapSet.new(optional)

    Enum.map(Enum.sort(allowed), fn eid ->
      auth_id = Map.get(bindings, eid)
      credless? = auth_id in [nil, ""] and MapSet.member?(optional_set, eid)

      %{
        entry_id: eid,
        label: registry_label(registry_entries, eid),
        credless?: credless?,
        has_binding: auth_id not in [nil, ""]
      }
    end)
  end

  defp registry_label(entries, eid) when is_list(entries) do
    Enum.find_value(entries, eid, fn e ->
      if entry_id(e) == eid, do: e["label"] || eid, else: nil
    end)
  end

  defp entry_id(e), do: e["entry_id"] |> to_string() |> String.trim()

  defp filtered_catalog_entries(entries, q) when is_list(entries) do
    q = q |> to_string() |> String.trim() |> String.downcase()

    if q == "" do
      entries
    else
      Enum.filter(entries, fn e ->
        id = entry_id(e) |> String.downcase()
        lab = (e["label"] || "") |> to_string() |> String.downcase()
        String.contains?(id, q) or String.contains?(lab, q)
      end)
    end
  end

  @impl true
  def render(assigns) do
    assigns =
      assign(assigns,
        connected: connected_rows(assigns.snap.policy.detail, assigns.snap.registry_entries),
        catalog_visible:
          ConnectCatalog.filter_personal_catalog_rows(
            assigns.snap.registry_entries,
            assigns.catalog_meta,
            assigns.catalog_meta_pending,
            assigns.oauth_ready_ids
          )
          |> filtered_catalog_entries(assigns.catalog_query)
      )

    ~H"""
    <div class="plasm-doc-stack">
      <div id="mcp-clipboard-bridge" class="hidden" phx-hook="McpClipboardBridge" phx-update="ignore">
      </div>
      <.page_header
        eyebrow="Configure"
        title="Connect APIs"
        subtitle="Policy and credentials console — linked catalogs, outbound auth, and MCP transport keys."
      >
        <:actions>
          <a class="plasm-button plasm-button-secondary" href="/traces">Session traces</a>
          <a class="plasm-button plasm-button-ghost" href="/oauth-apps">OAuth apps</a>
          <a class="plasm-button plasm-button-ghost" href="/">Station home</a>
        </:actions>
      </.page_header>

      <%= if @snap.policy.agent_detail_note do %>
        <.notice tone={if(@snap.policy.control_plane_ok, do: :warning, else: :danger)}>
          {@snap.policy.agent_detail_note}
        </.notice>
      <% end %>

      <%= if @snap.registry_error && not @snap.registry_ok do %>
        <.notice tone={:danger}>
          Registry unreachable: {@snap.registry_error}
        </.notice>
      <% end %>

      <%= if @last_provisioned_key do %>
        <div class="plasm-mcp-new-key-strip">
          <p>
            New MCP transport key — copy the Bearer token to your client configuration.
          </p>
          <div style="display:flex;flex-wrap:wrap;align-items:center;gap:0.35rem;">
            <.button type="button" variant={:secondary} phx-click="copy_last_provisioned_key">
              Copy bearer
            </.button>
            <.button variant={:ghost} phx-click="dismiss_key">Dismiss</.button>
          </div>
        </div>
      <% end %>

      <.panel id="connected-apps">
        <.section_header
          title="Connected apps"
          description="Allowlisted catalogs for this MCP endpoint. Disconnect removes them from policy."
        >
          <:actions>
            <.button variant={:primary} phx-click="open_catalog_modal">Open catalog</.button>
          </:actions>
        </.section_header>

        <%= if @connected == [] do %>
          <p class="plasm-stat-line" style="margin-top:0.85rem;">
            Connect your first app — apps appear here once linked (OAuth, API key, or public).
          </p>
          <div style="margin-top:0.75rem;">
            <.button variant={:secondary} phx-click="open_catalog_modal">Browse apps</.button>
          </div>
        <% else %>
          <ul class="plasm-stack" style="margin-top:0.85rem;list-style:none;padding:0;">
            <%= for row <- @connected do %>
              <li class="plasm-connected-card">
                <div class="plasm-connected-head">
                  <div class="plasm-stack" style="gap:0.35rem;">
                    <div style="display:flex;align-items:center;gap:0.5rem;">
                      <.provider_icon entry_id={row.entry_id} label={row.label} size={:md} />
                      <div>
                        <p class="plasm-section-kicker" style="margin:0;">{row.entry_id}</p>
                        <p style="margin:0;font-weight:650;">{row.label}</p>
                      </div>
                    </div>
                    <p class="plasm-stat-line">
                      <%= cond do %>
                        <% row.credless? -> %>
                          Public / credentialless
                        <% row.has_binding -> %>
                          Credentials stored
                        <% true -> %>
                          Enabled
                      <% end %>
                    </p>
                  </div>
                  <div class="plasm-connected-actions">
                    <a class="plasm-button plasm-button-secondary" href={"/tools/#{URI.encode(row.entry_id)}"}>
                      Explore
                    </a>
                    <.button variant={:ghost} phx-click="revoke_connected" phx-value-entry_id={row.entry_id}>
                      Disconnect
                    </.button>
                  </div>
                </div>
              </li>
            <% end %>
          </ul>
        <% end %>
      </.panel>

      <section class="plasm-connect-trace-peek" aria-label="Recent traces shortcut">
        <%= if @snap.traces_error do %>
          <p class="plasm-stat-line" style="margin:0;">
            Could not load trace snapshot: {@snap.traces_error}
            — try <a href="/traces">Session traces</a>.
          </p>
        <% else %>
          <% traces = @snap.traces || [] %>
          <% n = length(traces) %>
          <% tid =
            case List.first(traces) do
              row when is_map(row) -> row["trace_id"] || row[:trace_id] || ""
              _ -> ""
            end %>
          <% tid = if is_binary(tid), do: tid, else: to_string(tid || "") %>
          <p class="plasm-stat-line plasm-connect-trace-peek-line" style="margin:0;">
            <%= if n == 0 do %>
              No traces in the current snapshot — generate traffic, then
              <a href="/traces">open Session traces</a>.
            <% else %>
              Snapshot shows {n} newest trace<%= if n != 1, do: "s", else: "" %>
              <%= if tid != "" do %>
                · latest
                <a class="plasm-station-mono" href={"/traces/#{URI.encode(tid)}"}>
                  {String.slice(tid, 0, 12)}…
                </a>
              <% end %>
              · <a href="/traces">full history</a>
            <% end %>
          </p>
        <% end %>
      </section>

      <div class="plasm-stack" id="mcp-access-keys">
        <.panel>
          <details open class="plasm-mcp-api-keys-details">
            <summary style="font-size:0.875rem;font-weight:650;color:color-mix(in oklab, white 88%, transparent);">
              MCP transport keys
              <span class="plasm-mcp-api-keys-summary-hint">
                — copy, revoke, add (Bearer secrets for Streamable HTTP clients)
              </span>
            </summary>
            <div class="plasm-stack" style="margin-top:0.65rem;border-top:1px solid color-mix(in oklab, white 10%, transparent);padding-top:0.75rem;">
              <p class="plasm-stat-line" style="margin:0;font-size:0.8rem;">
                Secrets stay in plasm-mcp.
                <span class="font-medium">Copy</span>
                fetches the raw Bearer token and sends it to your clipboard — nothing extra is shown in the row.
              </p>
              <%= if @snap.policy.control_plane_ok and @snap.policy.config_id != nil do %>
                <div style="display:flex;flex-wrap:wrap;gap:0.5rem;">
                  <.button type="button" variant={:primary} phx-click="open_add_key_modal">
                    Add key
                  </.button>
                </div>
              <% end %>
              <%= if @snap.api_keys_error do %>
                <.notice tone={:danger}>{@snap.api_keys_error}</.notice>
              <% end %>
              <%= if @snap.api_keys != [] do %>
                <ul class="plasm-mcp-key-list">
                  <%= for row <- @snap.api_keys do %>
                    <% kid = row["key_id"] || row[:key_id] %>
                    <% row_id = to_string(kid || "") %>
                    <li class="plasm-mcp-key-card plasm-row-interactive">
                      <div class="plasm-mcp-key-meta">
                        <p class="plasm-mcp-key-label">{row_primary_title(row)}</p>
                        <%= if row_meta_line(row) do %>
                          <p class="plasm-mcp-key-fp">{row_meta_line(row)}</p>
                        <% end %>
                      </div>
                      <div class="plasm-mcp-key-actions">
                        <button
                          type="button"
                          class="plasm-button plasm-button-secondary plasm-mcp-key-text-btn"
                          phx-click="copy_mcp_access_key"
                          phx-value-key_id={row_id}
                        >
                          Copy bearer
                        </button>
                        <button
                          type="button"
                          class="plasm-button plasm-button-ghost plasm-mcp-key-text-btn plasm-mcp-key-text-btn--danger"
                          phx-click="revoke_access_key"
                          phx-value-key_id={row_id}
                        >
                          Revoke
                        </button>
                      </div>
                    </li>
                  <% end %>
                </ul>
              <% end %>
              <%= if @snap.api_keys == [] && @snap.api_keys_error == nil do %>
                <p class="plasm-stat-line" style="margin:0;">No API keys yet — use Add key.</p>
              <% end %>
            </div>
          </details>
        </.panel>
      </div>
    </div>

    <%= if @catalog_modal do %>
      <div class="plasm-modal-overlay">
        <div class="plasm-modal-card">
          <.section_header title="App catalog" description="Search and connect APIs discovered by your agent.">
            <:actions>
              <.button variant={:ghost} phx-click="close_catalog_modal">Close</.button>
            </:actions>
          </.section_header>

          <%= if @catalog_meta_loading? do %>
            <p class="plasm-stat-line" style="margin-top:0.75rem;">Loading catalog metadata…</p>
          <% end %>

          <form phx-change="catalog_search" id="connect-catalog-search" style="margin-top:0.75rem;">
            <label class="plasm-field" for="connect-catalog-q">
              <span>Search</span>
              <.control_input
                id="connect-catalog-q"
                type="search"
                name="q"
                value={@catalog_query}
                phx-debounce="200"
                autocomplete="off"
              />
            </label>
          </form>

          <ul class="plasm-stack" style="margin-top:0.85rem;list-style:none;padding:0;max-height:60vh;overflow:auto;">
            <%= for e <- @catalog_visible do %>
              <% eid = entry_id(e) %>
              <%= if eid != "" do %>
                <% meta = Map.get(@catalog_meta, eid, %{}) %>
                <% connected? = MapSet.member?(@snap.policy.selected_ids, eid) %>
                <% summary = ConnectCatalog.surface_summary_line(meta) %>
                <li class="plasm-connected-card" style="margin-bottom:0.65rem;">
                  <div class="plasm-connected-head">
                    <div class="plasm-stack" style="gap:0.35rem;">
                      <div style="display:flex;align-items:center;gap:0.5rem;">
                        <.provider_icon entry_id={eid} label={e["label"]} size={:md} />
                        <div>
                          <p class="plasm-section-kicker" style="margin:0;">{eid}</p>
                          <p style="margin:0;font-weight:650;">{e["label"] || eid}</p>
                        </div>
                      </div>
                      <%= if summary do %>
                        <p class="plasm-stat-line">{summary}</p>
                      <% end %>
                      <div class="plasm-tag-row">
                        <%= for {label, _tw} <- ConnectCatalog.auth_chip_rows_personal_meta(meta, eid, @oauth_ready_ids) do %>
                          <span class="plasm-tag">{label}</span>
                        <% end %>
                      </div>
                    </div>
                    <div class="plasm-connected-actions">
                      <%= if connected? do %>
                        <span class="plasm-status plasm-status-success">Connected</span>
                        <.button variant={:ghost} phx-click="revoke_connected" phx-value-entry_id={eid}>
                          Disconnect
                        </.button>
                      <% else %>
                        <.button
                          variant={:primary}
                          phx-click="catalog_connect"
                          phx-value-entry_id={eid}
                          disabled={@catalog_meta_loading?}
                        >
                          Connect
                        </.button>
                      <% end %>
                    </div>
                  </div>
                </li>
              <% end %>
            <% end %>
          </ul>
        </div>
      </div>
    <% end %>

    <%= if @api_key_modal do %>
      <div class="plasm-modal-overlay">
        <div class="plasm-modal-card">
          <.section_header
            title={"API key · #{@api_key_modal.label}"}
            description="Paste the secret this provider expects. It is stored on the agent vault and linked to this catalog."
          >
            <:actions>
              <.button variant={:ghost} phx-click="close_api_key_modal">Cancel</.button>
            </:actions>
          </.section_header>
          <form phx-submit="submit_api_key_connect" class="plasm-form-grid" style="margin-top:0.85rem;">
            <label class="plasm-field" style="grid-column: 1 / -1;">
              <span>API key / secret</span>
              <.control_input type="password" name="secret" value="" autocomplete="off" required />
            </label>
            <.button type="submit" variant={:primary}>Connect app</.button>
          </form>
        </div>
      </div>
    <% end %>

    <%= if @oauth_scope_modal do %>
      <div class="plasm-modal-overlay">
        <div class="plasm-modal-card" style="max-width:42rem;">
          <.section_header
            title="Review OAuth scopes"
            description={"#{@oauth_scope_modal.label} · choose scopes before connecting."}
          >
            <:actions>
              <.button variant={:ghost} phx-click="close_oauth_scope_modal">Cancel</.button>
            </:actions>
          </.section_header>
          <div style="margin-top:0.75rem;border-radius:0.5rem;padding:0.75rem;background:oklch(0.18 0.02 260);">
            <.oauth_scope_editor
              profile={@oauth_scope_modal.oauth_profile}
              scope_draft={@oauth_scope_modal.scope_draft}
              scope_query={@oauth_scope_modal.scope_query}
              query_event="appliance_scope_query"
              add_event="appliance_scope_add"
              remove_event="appliance_scope_remove"
              apply_set_event="appliance_scope_apply_set"
              query_input_id="appliance-scope-query"
            />
            <div style="margin-top:1rem;display:flex;justify-content:flex-end;gap:0.5rem;">
              <.button type="button" variant={:ghost} phx-click="close_oauth_scope_modal">Cancel</.button>
              <.button type="button" variant={:primary} phx-click="appliance_oauth_continue">
                Continue to OAuth
              </.button>
            </div>
          </div>
        </div>
      </div>
    <% end %>

    <%= if @add_key_modal do %>
      <div class="plasm-modal-overlay" phx-click="close_add_key_modal">
        <div class="plasm-modal-card" onclick="event.stopPropagation()">
          <.section_header
            title="Add MCP access key"
            description="Optional note for your records (stored on the agent). Leave blank for “Unnamed”."
          >
            <:actions>
              <.button variant={:ghost} phx-click="close_add_key_modal">Cancel</.button>
            </:actions>
          </.section_header>
          <form phx-submit="add_mcp_access_key" class="plasm-form-grid" style="margin-top:0.85rem;">
            <label class="plasm-field">
              <span>Note (optional)</span>
              <.control_input type="text" name="label" value="" autocomplete="off" placeholder="e.g. laptop, CI, …" />
            </label>
            <.button type="submit" variant={:primary}>Create key</.button>
          </form>
        </div>
      </div>
    <% end %>
    """
  end
end
