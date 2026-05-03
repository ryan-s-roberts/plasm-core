defmodule PlasmDesktop.Appliance.CatalogMeta do
  @moduledoc """
  Lightweight summaries from `GET /v1/registry/:entry_id/tool-model` for Connected API cards.
  """

  alias PlasmUiCore.ConnectProfile

  @spec from_tool_model(map()) :: map()
  def from_tool_model(body) when is_map(body) do
    overview = if is_map(body["overview"]), do: body["overview"], else: %{}
    scheme = get_in(body, ["auth", "scheme"])
    auth = if is_map(body["auth"]), do: body["auth"], else: %{}

    profile =
      case ConnectProfile.from_auth(auth) do
        {:ok, p} -> p
        _ -> nil
      end

    %{
      "auth_kinds" => auth_kinds(scheme),
      "auth_scheme" => scheme,
      "entity_count" => overview["entity_count"],
      "relation_edge_count" => overview["relation_edge_count"],
      "verb_count" => overview["verb_count"],
      "preview_lines" => preview_lines(body),
      "profile" => profile,
      "oauth_profile" => oauth_profile_from_tool_model(body)
    }
  end

  defp oauth_profile_from_tool_model(body) when is_map(body) do
    auth = body["auth"] || %{}
    oauth = if is_map(auth["oauth"]), do: auth["oauth"], else: %{}

    %{
      "oauth_provider" => oauth["provider"],
      "scope_entries" => if(is_map(oauth["scopes"]), do: oauth["scopes"], else: %{}),
      "default_scope_sets" =>
        if(is_map(oauth["default_scope_sets"]), do: oauth["default_scope_sets"], else: %{})
    }
  end

  defp auth_kinds("oauth2"), do: ["OAuth 2.0"]
  defp auth_kinds("api_key"), do: ["API key"]
  defp auth_kinds("none"), do: ["None"]
  defp auth_kinds(s) when is_binary(s) and s != "", do: [s]
  defp auth_kinds(_), do: []

  defp preview_lines(body) when is_map(body) do
    (body["entities"] || [])
    |> Enum.flat_map(fn e -> List.wrap(e["capabilities"]) end)
    |> Enum.map(& &1["description"])
    |> Enum.filter(&(is_binary(&1) and String.trim(&1) != ""))
    |> Enum.take(3)
  end
end
