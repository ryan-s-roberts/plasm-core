defmodule PlasmDesktopWeb.McpKeyUi do
  @moduledoc false

  @doc """
  Primary row title: operator-defined label when present, otherwise a stable short id + hint.
  """
  def row_primary_title(row) when is_map(row) do
    case normalize_label(row["label"] || row[:label]) do
      name when is_binary(name) ->
        name

      _ ->
        kid = row["key_id"] || row[:key_id]
        short = short_key_id(kid)

        if short != "" do
          "Access key · #{short}…"
        else
          "MCP access key"
        end
    end
  end

  @doc """
  Secondary line: fingerprint + created time (tabular, scannable).
  """
  def row_meta_line(row) when is_map(row) do
    fp =
      case row["key_fingerprint"] || row[:key_fingerprint] do
        f when is_binary(f) ->
          t = String.trim(f)
          if t != "", do: "Fingerprint #{t}", else: nil

        _ ->
          nil
      end

    created = format_created(row)

    case {fp, created} do
      {nil, nil} ->
        nil

      {f, nil} ->
        f

      {nil, c} ->
        c

      {f, c} ->
        "#{f} · #{c}"
    end
  end

  defp normalize_label(nil), do: nil

  defp normalize_label(l) do
    s = l |> to_string() |> String.trim()

    cond do
      s == "" -> nil
      String.downcase(s) == "unnamed" -> nil
      true -> s
    end
  end

  defp short_key_id(nil), do: ""

  defp short_key_id(kid) do
    s = kid |> to_string() |> String.trim()
    if s == "", do: "", else: String.slice(s, 0, 8)
  end

  defp format_created(row) do
    raw = row["created_at"] || row[:created_at]

    if is_binary(raw) and raw != "" do
      case DateTime.from_iso8601(String.trim(raw)) do
        {:ok, dt, _} ->
          "Created #{Calendar.strftime(dt, "%b %d, %Y · %H:%M UTC")}"

        _ ->
          nil
      end
    else
      nil
    end
  end
end
