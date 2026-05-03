defmodule PlasmUiCore.Web.ToolExplorerShared do
  @moduledoc false

  def parse_search_param(params, key \\ "q")

  def parse_search_param(params, key) when is_map(params) and is_binary(key) do
    case params do
      %{^key => v} when is_binary(v) -> v
      %{^key => v} -> to_string(v)
      _ -> ""
    end
  end

  def parse_search_param(_, _), do: ""

  def normalize_catalog_query_params(%{"mcp_config_id" => id}) when is_binary(id) do
    id = String.trim(id)
    if id == "", do: %{}, else: %{"mcp_config_id" => id}
  end

  def normalize_catalog_query_params(_), do: %{}

  def tool_entry_path(tool_base_path, entry_id, query_params \\ %{}) do
    query =
      query_params
      |> Enum.reject(fn {_k, v} -> is_nil(v) or v == "" end)
      |> Map.new()

    if map_size(query) == 0 do
      tool_base_path <> "/" <> entry_id
    else
      tool_base_path <> "/" <> entry_id <> "?" <> URI.encode_query(query)
    end
  end

  def catalog_path(tool_base_path, query_params) when map_size(query_params) == 0,
    do: tool_base_path

  def catalog_path(tool_base_path, query_params) do
    tool_base_path <> "?" <> URI.encode_query(query_params)
  end

  def tool_entity_href(tool_base_path, entry_id, entity_name, query_params)
      when is_binary(entity_name) do
    tool_entry_path(
      tool_base_path,
      entry_id,
      Map.merge(query_params, %{"entity" => entity_name, "focus" => "single"})
    )
  end

  def tool_entity_href(_tool_base_path, _entry_id, _, _), do: "#"

  def registry_row_id(e), do: e["entry_id"] || e[:entry_id]

  def registry_row_label(e), do: e["label"] || e[:label]

  def registry_row_tags(e) do
    case e["tags"] || e[:tags] do
      list when is_list(list) -> list
      _ -> []
    end
  end

  def catalog_matches?(entry, q) when is_binary(q) do
    q = String.downcase(String.trim(q))

    if q == "" do
      true
    else
      id = registry_row_id(entry) |> to_string() |> String.downcase()
      lab = registry_row_label(entry) |> to_string() |> String.downcase()

      tags =
        entry
        |> registry_row_tags()
        |> Enum.map(&to_string/1)
        |> Enum.join(" ")
        |> String.downcase()

      String.contains?(id, q) or String.contains?(lab, q) or String.contains?(tags, q)
    end
  end

  def filtered_registry_entries(entries, q) when is_list(entries) do
    Enum.filter(entries, &catalog_matches?(&1, q))
  end

  def mcp_scope_query_params(nil), do: %{}
  def mcp_scope_query_params(%{id: id}), do: %{"mcp_config_id" => id}

  def present?(nil), do: false
  def present?(""), do: false
  def present?(s) when is_binary(s), do: true
  def present?(_), do: false

  def present_args?(list) when is_list(list), do: list != []
  def present_args?(_), do: false

  def verb_kind_label("identity"), do: "Identity"
  def verb_kind_label("named_query"), do: "Named query"
  def verb_kind_label("named_search"), do: "Named search"

  def verb_kind_label(other) when is_binary(other) do
    other |> String.replace("_", " ") |> String.upcase()
  end

  def verb_kind_label(_), do: "—"

  def verb_kind_title("identity"), do: "GET by resource id (identity / id subpath)"
  def verb_kind_title(_), do: nil

  def explorer_entity_link_class do
    "no-underline inline border-b border-white/25 pb-px font-medium text-white/80 transition-colors hover:border-[oklch(0.62_0.12_252)]/55 hover:text-white"
  end

  def explorer_mono_entity_link_class do
    "no-underline inline border-b border-white/22 pb-px font-mono text-white/78 transition-colors hover:border-[oklch(0.62_0.12_252)]/50 hover:text-white"
  end

  def explorer_param_type_static_class do
    "inline-block max-w-full rounded-md border border-white/10 bg-black/[0.04] px-2 py-0.5 align-baseline font-mono text-[11px] leading-snug text-white/72"
  end

  def capability_line_kind_label("get"), do: "Get"
  def capability_line_kind_label("query"), do: "Query"
  def capability_line_kind_label("search"), do: "Search"
  def capability_line_kind_label("relation_nav"), do: "Relation"
  def capability_line_kind_label("method"), do: "Method"
  def capability_line_kind_label("other"), do: "Other"
  def capability_line_kind_label(other) when is_binary(other), do: String.upcase(other)
  def capability_line_kind_label(_), do: "—"

  def explorer_verb_shows_capability_id?(kind)
      when kind in ~w(create delete update action),
      do: true

  def explorer_verb_shows_capability_id?(_), do: false

  def ordered_capability_groups(groups) when is_map(groups) do
    order = ~w(get query search method other)
    rest = (Map.keys(groups) -- order) |> Enum.sort()

    for kind <- order ++ rest,
        rows = Map.get(groups, kind),
        is_list(rows),
        rows != [],
        do: {kind, rows}
  end

  def entity_sidebar_matches?(name, q) when is_binary(name) and is_binary(q) do
    q = String.downcase(String.trim(q))
    if q == "", do: true, else: String.contains?(String.downcase(name), q)
  end

  def entity_sidebar_matches?(_, _), do: true

  def filtered_entities(entities, q) when is_list(entities) do
    Enum.filter(entities, fn e ->
      name = e["name"] || e[:name] || ""
      entity_sidebar_matches?(to_string(name), q)
    end)
  end

  def filtered_entities(_, _), do: []
end
