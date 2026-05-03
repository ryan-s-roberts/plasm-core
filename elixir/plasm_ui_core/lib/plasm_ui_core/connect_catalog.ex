defmodule PlasmUiCore.ConnectCatalog do
  @moduledoc """
  Pure helpers for registry / catalog rows (auth chips, summaries) shared by SaaS and appliance UIs.
  """

  alias PlasmUiCore.ConnectPolicy
  alias PlasmUiCore.ConnectProfile

  @doc """
  Reads a decoded [`ConnectProfile`] stored under `meta["profile"]` (set when enriching catalog meta).
  """
  def connect_profile_from_meta(%{"profile" => %ConnectProfile{} = p}), do: p
  def connect_profile_from_meta(_), do: nil

  @doc false
  def surface_summary_line(meta) when is_map(meta) do
    with ec when is_integer(ec) <- meta["entity_count"],
         rc when is_integer(rc) <- meta["relation_edge_count"],
         vc when is_integer(vc) <- meta["verb_count"] do
      "#{ec} entities · #{rc} graph edges · #{vc} operations"
    else
      _ -> nil
    end
  end

  def surface_summary_line(_), do: nil

  @doc false
  def same_label?(label, entry_id) do
    String.downcase(to_string(label || "")) == String.downcase(to_string(entry_id || ""))
  end

  @doc """
  Builds `{label, tailwind_classes}` chip rows for personal registry views.
  """
  def auth_chip_rows_personal_meta(meta, entry_id, %MapSet{} = ops_ready) when is_map(meta) do
    eid = entry_id |> to_string() |> String.trim()
    profile = connect_profile_from_meta(meta)

    if profile == nil do
      [
        {"Connect profile unavailable", "border-white/10 bg-white/[0.03] text-white/50"}
      ]
    else
      {kinds, degraded?} =
        ConnectPolicy.display_auth_kinds_for_chips(
          profile,
          :personal,
          MapSet.member?(ops_ready, eid)
        )

      rows = auth_chip_rows(kinds)

      if degraded? do
        rows ++ [{"OAuth (needs Ops app)", "border-white/12 bg-white/[0.04] text-white/45"}]
      else
        rows
      end
    end
  end

  defp auth_chip_rows(auth_kinds) when is_list(auth_kinds) do
    kinds = auth_kinds |> Enum.map(&to_string/1) |> Enum.uniq()
    order = %{"oauth2" => 0, "api_key" => 1, "none" => 2}

    kinds =
      if kinds == [] do
        []
      else
        Enum.sort_by(kinds, fn k -> Map.get(order, k, 50) end)
      end

    Enum.map(kinds, &auth_chip_row/1)
  end

  defp auth_chip_row("oauth2"),
    do: {"OAuth 2.0", "border-violet-400/35 bg-violet-500/12 text-violet-100/95"}

  defp auth_chip_row("api_key"),
    do: {"API key", "border-amber-300/30 bg-amber-400/10 text-amber-100/95"}

  defp auth_chip_row("none"),
    do: {"Public (no credentials)", "border-slate-400/25 bg-slate-500/10 text-slate-100/90"}

  defp auth_chip_row(other) do
    label =
      other
      |> String.replace("_", " ")
      |> String.split()
      |> Enum.map(&String.capitalize/1)
      |> Enum.join(" ")

    {label, "border-white/15 bg-white/[0.05] text-white/78"}
  end

  @doc """
  Filters registry rows for personal catalog display (hides oauth-only rows without Ops readiness).
  """
  def filter_personal_catalog_rows(
        registry_entries,
        catalog_meta,
        catalog_meta_pending,
        %MapSet{} = ops_ready
      )
      when is_list(registry_entries) and is_map(catalog_meta) do
    pending = catalog_meta_pending || MapSet.new()

    registry_entries
    |> Enum.reject(fn row ->
      eid = registry_row_id(row) |> to_string()
      pending_meta? = MapSet.member?(pending, eid)

      if pending_meta? do
        false
      else
        meta = Map.get(catalog_meta, eid, %{})

        ConnectPolicy.hide_personal_catalog_row?(
          connect_profile_from_meta(meta),
          MapSet.member?(ops_ready, eid)
        )
      end
    end)
  end

  defp registry_row_id(row) when is_map(row), do: row["entry_id"] || row[:entry_id] || ""
end
