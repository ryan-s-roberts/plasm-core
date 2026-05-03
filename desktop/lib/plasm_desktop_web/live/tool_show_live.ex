defmodule PlasmDesktopWeb.ToolShowLive do
  @moduledoc """
  Per-catalog explorer: `GET /v1/registry/:entry_id/tool-model`.
  """
  use PlasmDesktopWeb, :live_view

  import PlasmUiCore.Web.ToolShow, only: [tool_show_body: 1]

  alias PlasmDesktopWeb.ToolExplorerShared

  @impl true
  def mount(params, session, socket) do
    entry_id = params["entry_id"]
    tool_base_path = ToolExplorerShared.tool_base_path_from_params(params)

    socket =
      socket
      |> assign(:entry_id, entry_id)
      |> assign(:tool_base_path, tool_base_path)
      |> assign(:catalog_query_params, %{})
      |> assign(:session_plug, session)
      |> assign(:page_title, "Tool explorer")
      |> assign(:model, nil)
      |> assign(:load_state, :loading)
      |> assign(:load_error, nil)
      |> assign(:selected, nil)
      |> assign(:url_entity, nil)
      |> assign(:url_focus, nil)
      |> assign(:entity_sidebar_query, "")

    {:ok, socket}
  end

  @impl true
  def handle_params(params, _uri, socket) do
    session = socket.assigns.session_plug
    entry_id = socket.assigns.entry_id
    url_entities = parse_entities(params)
    catalog_query_params = ToolExplorerShared.normalize_catalog_query_params(params)

    case PlasmDesktop.Mcp.DataPlane.fetch_tool_model(session, entry_id,
           focus: "all",
           entity: []
         ) do
      {:ok, model} ->
        selected = pick_selected_entity(model, url_entities)
        title = model["entry"]["label"] || entry_id

        {:noreply,
         socket
         |> assign(:load_state, :ok)
         |> assign(:load_error, nil)
         |> assign(:model, model)
         |> assign(:selected, selected)
         |> assign(:page_title, title)
         |> assign(:url_entity, List.first(url_entities))
         |> assign(:catalog_query_params, catalog_query_params)
         |> assign(:url_focus, params["focus"])}

      {:error, reason} ->
        {:noreply,
         socket
         |> assign(:load_state, :error)
         |> assign(
           :load_error,
           PlasmDesktop.Mcp.DataPlane.format_agent_http_error(session, reason)
         )
         |> assign(:model, nil)}
    end
  end

  defp parse_entities(params) do
    case params["entity"] do
      nil -> []
      e when is_binary(e) -> [e]
      list when is_list(list) -> Enum.map(list, &to_string/1)
    end
  end

  defp pick_selected_entity(model, url_entities) do
    names = Enum.map(model["entities"] || [], & &1["name"])

    case url_entities do
      [one | _] ->
        if one in names, do: one, else: List.first(names)

      _ ->
        List.first(names)
    end
  end

  @impl true
  def handle_event("select_entity", %{"name" => name}, socket) do
    entry_id = socket.assigns.entry_id
    base = socket.assigns.tool_base_path
    query_params = socket.assigns.catalog_query_params

    {:noreply,
     push_patch(socket,
       to:
         ToolExplorerShared.tool_entry_path(
           base,
           entry_id,
           Map.merge(query_params, %{"entity" => name, "focus" => "single"})
         )
     )}
  end

  def handle_event("show_all", _, socket) do
    entry_id = socket.assigns.entry_id
    base = socket.assigns.tool_base_path
    query_params = socket.assigns.catalog_query_params

    {:noreply,
     push_patch(socket, to: ToolExplorerShared.tool_entry_path(base, entry_id, query_params))}
  end

  def handle_event("entity_sidebar_search", params, socket) do
    q = ToolExplorerShared.parse_search_param(params, "q")

    {:noreply, assign(socket, :entity_sidebar_query, q)}
  end

  @impl true
  def render(assigns) do
    ~H"""
    <.tool_show_body
      load_state={@load_state}
      load_error={@load_error}
      model={@model}
      selected={@selected}
      url_focus={@url_focus}
      tool_base_path={@tool_base_path}
      entry_id={@entry_id}
      catalog_query_params={@catalog_query_params}
      entity_sidebar_query={@entity_sidebar_query}
    />
    """
  end
end
