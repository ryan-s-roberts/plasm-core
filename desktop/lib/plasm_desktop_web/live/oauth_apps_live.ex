defmodule PlasmDesktopWeb.OauthAppsLive do
  @moduledoc """
  Operator OAuth provider apps for the appliance: persisted in Postgres and synced to plasm-mcp
  (same flow as SaaS Ops OAuth provider).
  """
  use PlasmDesktopWeb, :live_view

  import Phoenix.HTML.Form, only: [input_value: 2]

  alias PlasmDesktop.Appliance.{
    OauthProviderApp,
    OauthProviderApps,
    RegistryOauth,
    YourMcpSnapshot
  }

  alias PlasmDesktop.Mcp.DataPlane

  @impl true
  def mount(_params, session, socket) do
    snap = YourMcpSnapshot.build(session)
    gen = 1

    socket =
      socket
      |> assign(:page_title, "OAuth provider apps")
      |> assign(:desk_session, session)
      |> assign(:snap, snap)
      |> assign(:oauth_apps, OauthProviderApps.list_oauth_provider_apps())
      |> assign(:oauth_capable_ids, MapSet.new())
      |> assign(:oauth_scan_loading?, snap.registry_entries != [])
      |> assign(:scan_gen, gen)
      |> assign(:entry_id, nil)
      |> assign(:app, nil)
      |> assign(:form, nil)
      |> assign(:oauth_tool_model_ok?, false)
      |> assign(:oauth_redirect_uri, oauth_redirect_uri(snap.http_base))
      |> assign(
        :oauth_redirect_uri_localhost,
        alternate_localhost_callback(oauth_redirect_uri(snap.http_base))
      )

    if snap.registry_entries != [] do
      send(self(), {:oauth_capable_scan, gen, session, snap.registry_entries})
    end

    {:ok, socket}
  end

  @impl true
  def handle_params(params, _uri, socket) do
    eid =
      case params["entry_id"] do
        s when is_binary(s) -> s |> URI.decode() |> String.trim()
        _ -> ""
      end

    cond do
      eid == "" ->
        {:noreply,
         socket
         |> assign(:page_title, "OAuth provider apps")
         |> assign(:entry_id, nil)
         |> assign(:app, nil)
         |> assign(:form, nil)
         |> assign(:oauth_tool_model_ok?, false)
         |> assign(:oauth_apps, OauthProviderApps.list_oauth_provider_apps())}

      true ->
        socket = assign(socket, :page_title, "OAuth · #{eid}")
        existing = OauthProviderApps.get_oauth_provider_app_by_entry(eid)

        oauth_ok? =
          case DataPlane.fetch_tool_model(socket.assigns.desk_session, eid, focus: "all") do
            {:ok, body} when is_map(body) ->
              RegistryOauth.tool_model_supports_outbound_oauth?(body)

            _ ->
              false
          end

        cond do
          existing != nil ->
            cs = OauthProviderApps.change_oauth_provider_app(existing)

            {:noreply,
             socket
             |> assign(:entry_id, eid)
             |> assign(:app, existing)
             |> assign(:form, to_form(cs, as: :oauth_app))
             |> assign(:oauth_tool_model_ok?, oauth_ok?)}

          oauth_ok? ->
            app = default_app(eid)
            cs = OauthProviderApps.change_oauth_provider_app(app)

            {:noreply,
             socket
             |> assign(:entry_id, eid)
             |> assign(:app, app)
             |> assign(:form, to_form(cs, as: :oauth_app))
             |> assign(:oauth_tool_model_ok?, true)}

          true ->
            {:noreply,
             socket
             |> put_flash(
               :error,
               "This catalog does not advertise OAuth2 outbound auth, or the tool-model could not be loaded."
             )
             |> push_navigate(to: "/oauth-apps")}
        end
    end
  end

  @impl true
  def handle_info({:oauth_capable_scan, gen, sess, entries}, socket) do
    if gen != socket.assigns.scan_gen do
      {:noreply, socket}
    else
      ready =
        entries
        |> Task.async_stream(
          fn row ->
            eid = registry_entry_id(row)

            if eid == "" do
              nil
            else
              case DataPlane.fetch_tool_model(sess, eid, focus: "all") do
                {:ok, body} when is_map(body) ->
                  if RegistryOauth.tool_model_supports_outbound_oauth?(body), do: eid, else: nil

                _ ->
                  nil
              end
            end
          end,
          max_concurrency: 8,
          timeout: 120_000
        )
        |> Enum.reduce(MapSet.new(), fn
          {:ok, eid}, acc when is_binary(eid) and eid != "" -> MapSet.put(acc, eid)
          _, acc -> acc
        end)

      {:noreply,
       socket
       |> assign(:oauth_capable_ids, ready)
       |> assign(:oauth_scan_loading?, false)}
    end
  end

  @impl true
  def handle_event("save", params, socket) do
    p = params["oauth_app"] || %{}

    secret =
      case params["client_secret"] do
        s when is_binary(s) -> String.trim(s)
        _ -> ""
      end

    eid = socket.assigns.entry_id

    enabled =
      case p["enabled"] do
        "true" -> true
        true -> true
        _ -> false
      end

    attrs = %{
      "entry_id" => eid,
      "provider" => norm_txt(p["provider"]),
      "authorization_endpoint" => norm_txt(p["authorization_endpoint"]),
      "token_endpoint" => norm_txt(p["token_endpoint"]),
      "client_id" => norm_txt(p["client_id"]),
      "client_secret_key" => norm_secret_key(p["client_secret_key"], eid),
      "redirect_uri_note" => norm_txt(p["redirect_uri_note"]),
      "docs_url" => norm_txt(p["docs_url"]),
      "enabled" => enabled
    }

    subject = "desktop"

    case OauthProviderApps.upsert_oauth_provider_app(attrs, subject) do
      {:ok, app} ->
        case OauthProviderApps.sync_oauth_provider_app_to_agent(
               socket.assigns.desk_session,
               app,
               secret
             ) do
          {:ok, app2} ->
            cs = OauthProviderApps.change_oauth_provider_app(app2)

            {:noreply,
             socket
             |> assign(:app, app2)
             |> assign(:form, to_form(cs, as: :oauth_app))
             |> assign(:oauth_apps, OauthProviderApps.list_oauth_provider_apps())
             |> put_flash(:info, "Saved and synced to plasm-mcp.")}

          {:error, reason} ->
            app_loaded = OauthProviderApps.get_oauth_provider_app_by_entry(eid) || app
            cs = OauthProviderApps.change_oauth_provider_app(app_loaded)

            {:noreply,
             socket
             |> assign(:app, app_loaded)
             |> assign(:form, to_form(cs, as: :oauth_app))
             |> assign(:oauth_apps, OauthProviderApps.list_oauth_provider_apps())
             |> put_flash(:error, "Saved locally but agent sync failed: #{format_err(reason)}")}
        end

      {:error, %Ecto.Changeset{} = cs} ->
        {:noreply,
         socket
         |> assign(form: to_form(cs, as: :oauth_app))
         |> put_flash(:error, "Fix the highlighted fields.")}
    end
  end

  def handle_event("resync_oauth_app", _, socket) do
    eid = socket.assigns.entry_id
    session = socket.assigns.desk_session

    case eid && OauthProviderApps.get_oauth_provider_app_by_entry(eid) do
      %OauthProviderApp{} = app ->
        case OauthProviderApps.sync_oauth_provider_app_to_agent(session, app, nil) do
          {:ok, app2} ->
            cs = OauthProviderApps.change_oauth_provider_app(app2)

            {:noreply,
             socket
             |> assign(:app, app2)
             |> assign(:form, to_form(cs, as: :oauth_app))
             |> assign(:oauth_apps, OauthProviderApps.list_oauth_provider_apps())
             |> put_flash(:info, "Pushed provider metadata to the agent again.")}

          {:error, reason} ->
            app_loaded = OauthProviderApps.get_oauth_provider_app_by_entry(eid) || app
            cs = OauthProviderApps.change_oauth_provider_app(app_loaded)

            {:noreply,
             socket
             |> assign(:app, app_loaded)
             |> assign(:form, to_form(cs, as: :oauth_app))
             |> put_flash(:error, "Sync failed: #{format_err(reason)}")}
        end

      _ ->
        {:noreply, put_flash(socket, :error, "No saved provider app for this catalog.")}
    end
  end

  def handle_event("copy_oauth_redirect_uri", _, socket) do
    uri = socket.assigns.oauth_redirect_uri || ""

    if uri != "" do
      {:noreply, push_event(socket, "mcp:copy", %{text: uri})}
    else
      {:noreply, socket}
    end
  end

  defp default_app(eid) do
    %OauthProviderApp{
      entry_id: eid,
      client_secret_key: OauthProviderApp.secret_key_for_entry(eid),
      enabled: true,
      client_id: ""
    }
  end

  defp norm_txt(v) do
    case v do
      nil -> ""
      s -> s |> to_string() |> String.trim()
    end
  end

  defp norm_secret_key(v, eid) do
    case norm_txt(v) do
      "" -> OauthProviderApp.secret_key_for_entry(eid)
      s -> s
    end
  end

  defp format_err({:http_status, status, body}), do: "HTTP #{status}: #{inspect(body)}"
  defp format_err(other), do: inspect(other)

  defp registry_entry_id(row) when is_map(row) do
    (row["entry_id"] || row[:entry_id] || "")
    |> to_string()
    |> String.trim()
  end

  defp oauth_redirect_uri(http_base) when is_binary(http_base) do
    base = http_base |> String.trim() |> String.trim_trailing("/")
    uri = URI.parse(base)

    case uri do
      %URI{scheme: sch, host: h} when is_binary(sch) and is_binary(h) ->
        port =
          cond do
            uri.port in [nil, URI.default_port(sch)] -> ""
            true -> ":#{uri.port}"
          end

        "#{sch}://#{h}#{port}/oauth/link/callback"

      _ ->
        if base != "", do: base <> "/oauth/link/callback", else: ""
    end
  end

  defp oauth_redirect_uri(_), do: ""

  defp alternate_localhost_callback(uri) when is_binary(uri) do
    if String.contains?(uri, "127.0.0.1") do
      String.replace(uri, "127.0.0.1", "localhost", global: false)
    else
      nil
    end
  end

  defp alternate_localhost_callback(_), do: nil

  defp registry_label(entries, eid) when is_list(entries) do
    Enum.find_value(entries, eid, fn row ->
      if registry_entry_id(row) == eid, do: row["label"] || row[:label] || eid, else: nil
    end)
  end

  @impl true
  def render(assigns) do
    assigns = assign(assigns, :configured_ids, Map.new(assigns.oauth_apps, &{&1.entry_id, true}))

    ~H"""
    <div class="plasm-doc-stack">
      <div id="mcp-clipboard-bridge" class="hidden" phx-hook="McpClipboardBridge" phx-update="ignore">
      </div>

      <%= if @entry_id == nil do %>
        <.page_header
          eyebrow="Configure"
          title="OAuth provider apps"
          subtitle="Register client IDs and OAuth endpoints per catalog. Metadata stays in appliance Postgres; secrets sync to the agent KV."
        >
          <:actions>
            <a class="plasm-button plasm-button-secondary" href="/connect-apis">Connect APIs</a>
            <a class="plasm-button plasm-button-ghost" href="/">Station home</a>
          </:actions>
        </.page_header>

        <.panel>
          <.section_header
            title="Configured apps"
            description="Saved in appliance Postgres and synced to plasm-mcp when you save."
          />
          <%= if @oauth_apps == [] do %>
            <p class="plasm-stat-line" style="margin-top:0.75rem;">No provider apps yet — pick a catalog below.</p>
          <% else %>
            <ul class="plasm-stack" style="margin-top:0.85rem;list-style:none;padding:0;">
              <%= for app <- @oauth_apps do %>
                <li class="plasm-connected-card">
                  <div class="plasm-connected-head">
                    <div class="plasm-stack" style="gap:0.25rem;">
                      <p style="margin:0;font-weight:650;">{app.entry_id}</p>
                      <p class="plasm-stat-line">
                        <%= if app.enabled do %>
                          <span class="plasm-status plasm-status-success">Enabled</span>
                        <% else %>
                          <span class="plasm-status plasm-status-muted">Disabled</span>
                        <% end %>
                        <%= if app.last_synced_at do %>
                          · last sync {Calendar.strftime(app.last_synced_at, "%Y-%m-%d %H:%M:%SZ")}
                        <% end %>
                      </p>
                    </div>
                    <div class="plasm-connected-actions">
                      <a class="plasm-button plasm-button-secondary" href={"/oauth-apps/#{URI.encode(app.entry_id)}"}>
                        Edit
                      </a>
                    </div>
                  </div>
                </li>
              <% end %>
            </ul>
          <% end %>
        </.panel>

        <.panel>
          <.section_header
            title="OAuth-capable catalogs"
            description="Only catalogs whose tool-model advertises outbound OAuth2 appear here."
          />
          <%= if @oauth_scan_loading? do %>
            <p class="plasm-stat-line" style="margin-top:0.75rem;">Scanning registry for OAuth metadata…</p>
          <% else %>
            <%= if MapSet.size(@oauth_capable_ids) == 0 do %>
              <p class="plasm-stat-line" style="margin-top:0.75rem;">
                No OAuth-capable catalogs found (or registry unreachable).
              </p>
            <% else %>
              <ul class="plasm-stack" style="margin-top:0.85rem;list-style:none;padding:0;">
                <%= for eid <- @oauth_capable_ids |> MapSet.to_list() |> Enum.sort() do %>
                  <% label = registry_label(@snap.registry_entries, eid) %>
                  <li class="plasm-connected-card">
                    <div class="plasm-connected-head">
                      <div class="plasm-stack" style="gap:0.25rem;">
                        <p class="plasm-section-kicker" style="margin:0;">{eid}</p>
                        <p style="margin:0;font-weight:650;">{label}</p>
                      </div>
                      <div class="plasm-connected-actions">
                        <a class="plasm-button plasm-button-primary" href={"/oauth-apps/#{URI.encode(eid)}"}>
                          <%= if Map.has_key?(@configured_ids, eid), do: "Edit", else: "Configure" %>
                        </a>
                      </div>
                    </div>
                  </li>
                <% end %>
              </ul>
            <% end %>
          <% end %>
        </.panel>
      <% else %>
        <.page_header
          eyebrow="Configure"
          title={@entry_id}
          subtitle="Scopes are chosen when you connect an account from Connect APIs — not here."
        >
          <:actions>
            <a class="plasm-button plasm-button-ghost" href="/oauth-apps">← All apps</a>
          </:actions>
        </.page_header>

        <%= if @app && @app.last_sync_error do %>
          <.notice tone={:danger}>
            Last sync error: {@app.last_sync_error}
          </.notice>
        <% end %>

        <%= if @app && not @oauth_tool_model_ok? do %>
          <.notice tone={:warning}>
            Tool-model OAuth metadata is unavailable right now — you can still edit the saved record.
          </.notice>
        <% end %>

        <.panel>
          <.section_header title="OAuth redirect URI (agent)" description="Register this exact URL with your identity provider." />
          <div class="plasm-notice plasm-notice-warning" style="margin-top:0.75rem;">
            <p class="m-0 text-sm">
              The agent sends this as <code class="font-mono">redirect_uri</code>. It must match
              <code class="font-mono">PLASM_OAUTH_LINK_REDIRECT_URI</code> on plasm-mcp when set.
              Computed from your session HTTP base — adjust agent env if you terminate TLS elsewhere.
            </p>
            <div style="margin-top:0.65rem;display:flex;flex-wrap:wrap;gap:0.5rem;align-items:center;">
              <code class="font-mono text-xs" style="word-break:break-all;flex:1;min-width:12rem;">
                {@oauth_redirect_uri}
              </code>
              <.button type="button" variant={:secondary} phx-click="copy_oauth_redirect_uri">
                Copy
              </.button>
            </div>
            <%= if @oauth_redirect_uri_localhost do %>
              <p class="plasm-stat-line" style="margin-top:0.5rem;">
                Optional alias (localhost vs 127.0.0.1): <code class="font-mono text-xs">{@oauth_redirect_uri_localhost}</code>
              </p>
            <% end %>
          </div>
        </.panel>

        <.panel>
          <.form :let={f} for={@form} id="oauth-app-form" phx-submit="save" class="plasm-stack" style="gap:0.85rem;">
            <p class="plasm-stat-line m-0">
              KV secret key: <code class="font-mono">{@app.client_secret_key}</code>
            </p>

            <label class="plasm-field">
              <span>Client secret KV key</span>
              <.control_input
                type="text"
                name={f[:client_secret_key].name}
                id={f[:client_secret_key].id}
                value={input_value(f, :client_secret_key)}
                autocomplete="off"
              />
            </label>
            <p class="plasm-stat-line m-0" style="font-size:0.8rem;">
              Must start with <code class="font-mono">plasm:oauth_app:v1:</code>
              or <code class="font-mono">plasm:outbound:</code>.
            </p>

            <label class="plasm-field">
              <span>Provider label (optional)</span>
              <.control_input
                type="text"
                name={f[:provider].name}
                id={f[:provider].id}
                value={input_value(f, :provider)}
                autocomplete="off"
              />
            </label>

            <label class="plasm-field">
              <span>Authorization endpoint</span>
              <.control_input
                type="text"
                name={f[:authorization_endpoint].name}
                id={f[:authorization_endpoint].id}
                value={input_value(f, :authorization_endpoint)}
                autocomplete="off"
              />
            </label>

            <label class="plasm-field">
              <span>Token endpoint</span>
              <.control_input
                type="text"
                name={f[:token_endpoint].name}
                id={f[:token_endpoint].id}
                value={input_value(f, :token_endpoint)}
                autocomplete="off"
              />
            </label>

            <label class="plasm-field">
              <span>Client ID</span>
              <.control_input
                type="text"
                name={f[:client_id].name}
                id={f[:client_id].id}
                value={input_value(f, :client_id)}
                autocomplete="off"
                required
              />
            </label>

            <label class="plasm-field">
              <span>Client secret (optional)</span>
              <.control_input
                type="password"
                name="client_secret"
                id="oauth-app-client-secret"
                value=""
                autocomplete="off"
                placeholder="Paste only when setting or rotating; stored in agent KV at the key above"
              />
            </label>

            <label class="plasm-field">
              <span>Redirect URI note (optional)</span>
              <textarea
                id={f[:redirect_uri_note].id}
                name={f[:redirect_uri_note].name}
                class="plasm-input"
                rows="3"
              >{input_value(f, :redirect_uri_note)}</textarea>
            </label>

            <label class="plasm-field">
              <span>Docs URL (optional)</span>
              <.control_input
                type="url"
                name={f[:docs_url].name}
                id={f[:docs_url].id}
                value={input_value(f, :docs_url)}
                autocomplete="off"
              />
            </label>

            <label class="plasm-field" style="display:flex;align-items:center;gap:0.5rem;">
              <.control_input
                type="checkbox"
                name={f[:enabled].name}
                id={f[:enabled].id}
                value="true"
                checked={input_value(f, :enabled) == true}
              />
              <span>Enabled (runtime override active on agent)</span>
            </label>

            <div class="plasm-page-actions" style="gap:0.5rem;">
              <.button type="submit" variant={:primary}>Save &amp; sync to agent</.button>
              <.button type="button" variant={:secondary} phx-click="resync_oauth_app">
                Push to agent again
              </.button>
            </div>
          </.form>
        </.panel>
      <% end %>
    </div>
    """
  end
end
