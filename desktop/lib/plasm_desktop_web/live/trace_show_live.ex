defmodule PlasmDesktopWeb.TraceShowLive do
  use PlasmDesktopWeb, :live_view

  alias PlasmDesktop.Mcp.DataPlane

  @impl true
  def mount(%{"trace_id" => tid}, session, socket) do
    tid = URI.decode(tid)

    {st, body, err} =
      case DataPlane.fetch_trace_detail(session, tid) do
        {:ok, m} -> {:ok, m, nil}
        {:error, :not_found} -> {:error, nil, "Trace not found."}
        {:error, r} -> {:error, nil, DataPlane.format_client_error(r)}
      end

    {:ok,
     socket
     |> assign(:page_title, "Trace #{String.slice(tid, 0, 8)}…")
     |> assign(:desk_session, session)
     |> assign(:trace_id, tid)
     |> assign(:load_state, st)
     |> assign(:detail, body)
     |> assign(:load_error, err)}
  end

  @impl true
  def render(assigns) do
    ~H"""
    <div class="plasm-doc-stack">
      <.page_header eyebrow="Observe" title={@page_title} subtitle="Raw agent payload for debugging.">
        <:actions>
          <a class="plasm-button plasm-button-ghost" href="/traces">← All traces</a>
        </:actions>
      </.page_header>

      <%= if @load_state == :error do %>
        <.notice tone={:danger}>{@load_error}</.notice>
      <% end %>

      <%= if @load_state == :ok do %>
        <.panel>
          <pre style="white-space: pre-wrap; word-break: break-word;"><%= detail_json(@detail) %></pre>
        </.panel>
      <% end %>
    </div>
    """
  end

  defp detail_json(nil), do: ""
  defp detail_json(map) when is_map(map), do: Jason.encode!(map)

  defp detail_json(other), do: inspect(other, limit: :infinity, printable_limit: :infinity)
end
