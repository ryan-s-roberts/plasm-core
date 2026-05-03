defmodule PlasmDesktopWeb.TracesLive do
  use PlasmDesktopWeb, :live_view

  alias PlasmDesktop.Mcp.DataPlane

  @page_size 8

  @impl true
  def mount(_params, session, socket) do
    socket =
      socket
      |> assign(:page_title, "Session traces")
      |> assign(:desk_session, session)
      |> assign(:page, 1)
      |> assign(:traces, [])
      |> assign(:has_next, false)
      |> assign(:error, nil)

    {:ok, load_page(socket)}
  end

  defp load_page(socket) do
    session = socket.assigns.desk_session
    page = max(socket.assigns.page, 1)
    offset = (page - 1) * @page_size

    case DataPlane.list_traces(session, offset: offset, limit: @page_size + 1) do
      {:ok, %{"traces" => rows}} when is_list(rows) ->
        has_next = length(rows) > @page_size
        page_rows = Enum.take(rows, @page_size)

        socket
        |> assign(:page, page)
        |> assign(:traces, page_rows)
        |> assign(:has_next, has_next)
        |> assign(:error, nil)

      {:ok, body} when is_map(body) ->
        rows = Map.get(body, "traces") || []
        has_next = length(rows) > @page_size
        page_rows = Enum.take(rows, @page_size)

        socket
        |> assign(:page, page)
        |> assign(:traces, page_rows)
        |> assign(:has_next, has_next)
        |> assign(:error, nil)

      {:error, reason} ->
        assign(socket,
          traces: [],
          has_next: false,
          error: DataPlane.format_client_error(reason)
        )
    end
  end

  @impl true
  def handle_event("prev_page", _, socket) do
    page = max(socket.assigns.page - 1, 1)
    {:noreply, load_page(assign(socket, :page, page))}
  end

  def handle_event("next_page", _, socket) do
    page = socket.assigns.page + 1
    {:noreply, load_page(assign(socket, :page, page))}
  end

  @impl true
  def render(assigns) do
    ~H"""
    <div class="plasm-doc-stack">
      <.page_header
        eyebrow="Observe"
        title="Session traces"
        subtitle="Recent agent runs for this MCP endpoint (GET /v1/traces)."
      >
        <:actions>
          <a class="plasm-button plasm-button-secondary" href="/connect-apis">Connect APIs</a>
          <a class="plasm-button plasm-button-ghost" href="/">Station home</a>
        </:actions>
      </.page_header>

      <%= if @error do %>
        <.notice tone={:danger}>{@error}</.notice>
      <% end %>

      <%= if @traces == [] && @error == nil do %>
        <.panel>
          <p class="plasm-stat-line">No traces on this page.</p>
        </.panel>
      <% end %>

      <%= if @traces != [] do %>
        <.panel>
          <.trace_table traces={@traces} />
          <div class="plasm-catalog-toolbar" style="margin-top:0.85rem;">
            <p class="plasm-stat-line">Page {@page}</p>
            <div class="plasm-page-actions">
              <.button
                type="button"
                variant={:secondary}
                disabled={@page <= 1}
                phx-click="prev_page"
              >
                Previous
              </.button>
              <.button
                type="button"
                variant={:secondary}
                disabled={not @has_next}
                phx-click="next_page"
              >
                Next
              </.button>
            </div>
          </div>
        </.panel>
      <% end %>
    </div>
    """
  end
end
