defmodule PlasmDesktopWeb.YourMcpLive do
  @moduledoc """
  Appliance **Station home**: health readout and fast paths into Connect APIs and Session traces.
  Policy detail, keys, and trace tables live on those routes — not duplicated here.
  """
  use PlasmDesktopWeb, :live_view

  alias PlasmDesktop.Appliance.{DefaultMcpAccessKey, YourMcpSnapshot}

  @impl true
  def mount(_params, session, socket) do
    snap = YourMcpSnapshot.build(session)

    socket =
      socket
      |> assign(:page_title, "Your MCP")
      |> assign(:desk_session, session)
      |> assign(:snap, snap)
      |> assign(:last_provisioned_key, nil)

    {:ok, apply_default_access_key(socket)}
  end

  @impl true
  def handle_event("dismiss_key", _, socket) do
    {:noreply, assign(socket, :last_provisioned_key, nil)}
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

  defp apply_default_access_key(socket) do
    session = socket.assigns.desk_session

    case DefaultMcpAccessKey.bootstrap(session) do
      {:ok, key, snap} ->
        socket
        |> assign(:snap, snap)
        |> assign(:last_provisioned_key, key)
        |> put_flash(
          :info,
          "Default access key created — copy from the banner, then manage keys on Connect APIs."
        )

      {:noop, snap} ->
        assign(socket, :snap, snap)

      {:error, reason, snap} ->
        require Logger

        Logger.warning("[plasm_desktop] default MCP access key: #{inspect(reason, limit: 120)}")

        assign(socket, :snap, snap)
    end
  end

  @impl true
  def render(assigns) do
    n_apis = MapSet.size(assigns.snap.policy.selected_ids)
    n_keys = length(assigns.snap.api_keys || [])
    assigns = assign(assigns, n_apis: n_apis, n_keys: n_keys)

    ~H"""
    <div class="plasm-doc-stack">
      <div id="mcp-clipboard-bridge" class="hidden" phx-hook="McpClipboardBridge" phx-update="ignore">
      </div>
      <.page_header
        eyebrow="Station"
        title="Your MCP"
        subtitle="Health readout only — policy, credentials, and traces live on Connect APIs and Session traces."
      />

      <.station_status_strip snap={@snap} />

      <%= if @last_provisioned_key do %>
        <div class="plasm-mcp-new-key-strip">
          <p>
            New MCP transport key — copy now, then open Connect APIs to manage keys long-term.
          </p>
          <div style="display:flex;flex-wrap:wrap;align-items:center;gap:0.35rem;">
            <.button type="button" variant={:secondary} phx-click="copy_last_provisioned_key">
              Copy bearer
            </.button>
            <.button variant={:ghost} phx-click="dismiss_key">Dismiss</.button>
          </div>
        </div>
      <% end %>

      <section class="plasm-hub-ops" aria-label="Primary workflows">
        <a class="plasm-button plasm-button-primary plasm-hub-ops-btn" href="/connect-apis">
          Connect APIs
        </a>
        <a class="plasm-button plasm-button-secondary plasm-hub-ops-btn" href="/traces">
          Session traces
        </a>
      </section>

      <p class="plasm-hub-summary plasm-stat-line">
        {@n_apis} APIs linked · {@n_keys} transport keys ·
        <a href="/connect-apis">Manage policy and keys</a>
        ·
        <a href="/traces">Browse trace history</a>
      </p>

      <p class="plasm-hub-aux plasm-stat-line">
        Also:
        <a href="/tools">Tool catalog</a>
        ·
        <a href="/oauth-apps">OAuth apps</a>
        ·
        <a href="/settings">Connection settings</a>
      </p>
    </div>
    """
  end
end
