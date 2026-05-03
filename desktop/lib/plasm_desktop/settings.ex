defmodule PlasmDesktop.Settings do
  @moduledoc """
  Local appliance chrome in `desktop_settings` (URLs, tokens, policy pointers).

  MCP allowlists remain canonical on the agent (`project_mcp_*`), not here.
  """

  alias PlasmDesktop.Repo
  alias PlasmDesktop.Settings.DesktopSetting

  @spec get_all_map() :: map()
  def get_all_map do
    Repo.all(DesktopSetting)
    |> Map.new(fn %DesktopSetting{key: k, value: v} -> {k, v || ""} end)
  end

  @spec upsert_many(map()) :: :ok | {:error, term()}
  def upsert_many(attrs) when is_map(attrs) do
    rows =
      Enum.map(attrs, fn {k, v} ->
        %{key: to_string(k), value: to_string(v || "")}
      end)

    Repo.insert_all(
      DesktopSetting,
      rows,
      on_conflict: {:replace, [:value]},
      conflict_target: [:key]
    )

    :ok
  rescue
    e in Postgrex.Error -> {:error, e}
  end
end
