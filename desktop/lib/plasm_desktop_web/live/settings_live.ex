defmodule PlasmDesktopWeb.SettingsLive do
  @moduledoc """
  Appliance connection chrome persisted to `desktop_settings` (URLs + optional auth headers).

  Effective values are merged into the Plug session by [`DesktopSessionPlug`](`PlasmDesktopWeb.DesktopSessionPlug`).
  """
  use PlasmDesktopWeb, :live_view

  alias PlasmDesktop.Mcp.DataPlane
  alias PlasmDesktop.Settings

  @impl true
  def mount(_params, session, socket) do
    db = Settings.get_all_map()

    {:ok,
     socket
     |> assign(:page_title, "Connection settings")
     |> assign(:desk_session, session)
     |> assign(:http_base, Map.get(db, "plasm_mcp_http_base_url", ""))
     |> assign(:public_base, Map.get(db, "plasm_mcp_public_base_url", ""))
     |> assign(:bearer, Map.get(db, "plasm_bearer_token", ""))
     |> assign(:api_key, Map.get(db, "plasm_api_key", ""))}
  end

  @impl true
  def handle_event("save", params, socket) do
    attrs = %{
      "plasm_mcp_http_base_url" => trim(params["http_base"]),
      "plasm_mcp_public_base_url" => trim(params["public_base"]),
      "plasm_bearer_token" => trim(params["bearer"]),
      "plasm_api_key" => trim(params["api_key"])
    }

    case Settings.upsert_many(attrs) do
      :ok ->
        {:noreply,
         socket
         |> put_flash(:info, "Saved settings — reload pages to pick up new agent endpoints.")
         |> push_navigate(to: "/settings")}

      {:error, reason} ->
        {:noreply, put_flash(socket, :error, "Save failed: #{inspect(reason, limit: 160)}")}
    end
  end

  @impl true
  def handle_event("test_connection", _params, socket) do
    session = socket.assigns.desk_session

    socket =
      case DataPlane.list_registry(session) do
        {:ok, %{"entries" => entries}} when is_list(entries) ->
          n = length(entries)

          put_flash(
            socket,
            :info,
            "Discovery OK at #{DataPlane.base_url(session)} — #{n} registry entr#{if n == 1, do: "y", else: "ies"}."
          )

        {:ok, other} ->
          put_flash(
            socket,
            :error,
            "Registry reachable but shape unexpected: #{inspect(other, limit: 120)}"
          )

        {:error, reason} ->
          put_flash(socket, :error, DataPlane.format_agent_http_error(session, reason))
      end

    {:noreply, socket}
  end

  defp trim(nil), do: ""
  defp trim(v) when is_binary(v), do: String.trim(v)
  defp trim(v), do: v |> to_string() |> String.trim()

  @impl true
  def render(assigns) do
    ~H"""
    <div class="plasm-doc-stack">
      <.page_header
        eyebrow="Configure"
        title="Connection settings"
        subtitle="Env overrides win over saved values (PLASM_MCP_HTTP_BASE_URL, PLASM_MCP_PUBLIC_BASE_URL, PLASM_DESKTOP_BEARER_TOKEN, PLASM_DESKTOP_API_KEY)."
      >
        <:actions>
          <a class="plasm-button plasm-button-secondary" href="/oauth-apps">OAuth apps</a>
          <a class="plasm-button plasm-button-ghost" href="/">Station home</a>
        </:actions>
      </.page_header>

      <p class="plasm-stat-line" style="margin:0 0 0.5rem 0;">
        Outbound OAuth client IDs and endpoints:
        <a href="/oauth-apps">OAuth provider apps</a>.
      </p>

      <.panel>
        <.section_header
          title="Agent endpoints"
          description="Discovery HTTP base powers GET /v1/registry; Streamable MCP URL is what clients connect to."
        />
        <form phx-submit="save" class="plasm-form-grid" style="margin-top:0.85rem;">
          <label class="plasm-field">
            <span>Agent HTTP base</span>
            <.control_input type="url" name="http_base" value={@http_base} placeholder="http://127.0.0.1:3000" />
          </label>
          <label class="plasm-field">
            <span>Public MCP base</span>
            <.control_input
              type="url"
              name="public_base"
              value={@public_base}
              placeholder="http://127.0.0.1:3001/mcp"
            />
          </label>
          <label class="plasm-field">
            <span>Bearer token (execute / traces)</span>
            <.control_input type="password" name="bearer" value={@bearer} autocomplete="off" />
          </label>
          <label class="plasm-field">
            <span>API key (optional alternate)</span>
            <.control_input type="password" name="api_key" value={@api_key} autocomplete="off" />
          </label>
          <div class="plasm-page-actions" style="grid-column: 1 / -1; gap: 0.5rem;">
            <.button type="submit" variant={:primary}>Save</.button>
            <.button type="button" variant={:secondary} phx-click="test_connection">
              Test discovery
            </.button>
          </div>
          <p class="plasm-stat-line" style="grid-column: 1 / -1; margin: 0;">
            Test discovery calls <code class="font-mono">GET …/v1/registry</code> using the
            <strong>effective</strong> session values shown below (save first if you changed URLs).
          </p>
        </form>
      </.panel>

      <.panel>
        <.section_header title="Effective session values" description="What LiveViews resolved this request." />
        <div class="plasm-stack" style="margin-top:0.75rem;">
          <.value_row label="HTTP base" value={@desk_session["plasm_mcp_http_base_url"] || "—"} />
          <.value_row label="Public MCP" value={@desk_session["plasm_mcp_public_base_url"] || "—"} />
        </div>
      </.panel>
    </div>
    """
  end
end
