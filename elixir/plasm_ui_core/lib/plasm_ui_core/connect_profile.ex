defmodule PlasmUiCore.ConnectProfile do
  @moduledoc """
  Decodes the tool-model `auth.connect_profile` object (typed CGS projection from plasm-agent).

  Callers must use this — there is no alternate stringly path.
  """

  @enforce_keys [:capability, :oauth, :has_public_mode, :has_api_key, :has_oauth]
  defstruct @enforce_keys

  @type capability :: :public | :api_key_only | :oauth_only | :api_key_and_oauth
  @type t :: %__MODULE__{
          capability: capability(),
          oauth: %{provider_present: boolean(), scope_catalog_present: boolean()},
          has_public_mode: boolean(),
          has_api_key: boolean(),
          has_oauth: boolean()
        }

  @doc """
  Decodes `auth` from a tool-model JSON map (`%{"auth" => ...}`).

  Returns `:error` when `connect_profile` is missing or invalid.
  """
  def from_auth(%{} = auth) do
    case auth["connect_profile"] do
      %{} = p -> {:ok, from_connect_profile_map(p)}
      _ -> :error
    end
  end

  def from_auth(_), do: :error

  def from_auth!(auth) do
    case from_auth(auth) do
      {:ok, p} -> p
      :error -> raise ArgumentError, "tool-model auth.connect_profile is required"
    end
  end

  defp from_connect_profile_map(p) do
    oauth = Map.get(p, "oauth") || %{}

    %__MODULE__{
      capability: parse_capability(Map.get(p, "capability")),
      oauth: %{
        provider_present: truthy?(Map.get(oauth, "provider_present")),
        scope_catalog_present: truthy?(Map.get(oauth, "scope_catalog_present"))
      },
      has_public_mode: truthy?(Map.get(p, "has_public_mode")),
      has_api_key: truthy?(Map.get(p, "has_api_key")),
      has_oauth: truthy?(Map.get(p, "has_oauth"))
    }
  end

  defp parse_capability(v) do
    case to_string(v || "") |> String.trim() |> String.downcase() do
      "public" -> :public
      "api_key_only" -> :api_key_only
      "oauth_only" -> :oauth_only
      "api_key_and_oauth" -> :api_key_and_oauth
      _ -> :public
    end
  end

  defp truthy?(v) when v in [true, "true", 1, "1"], do: true
  defp truthy?(_), do: false

  @doc "Normalized kind tags for UI chips (order: oauth2, api_key, none)."
  def to_auth_kinds_list(%__MODULE__{} = p) do
    []
    |> then(fn ks -> if p.has_oauth, do: ks ++ ["oauth2"], else: ks end)
    |> then(fn ks -> if p.has_api_key, do: ks ++ ["api_key"], else: ks end)
    |> then(fn ks -> if p.has_public_mode, do: ks ++ ["none"], else: ks end)
    |> Enum.uniq()
  end
end
