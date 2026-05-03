defmodule PlasmDesktop.Appliance.RegistryOauth do
  @moduledoc false

  alias PlasmUiCore.ConnectProfile

  @doc """
  True when the loaded CGS tool-model exposes OAuth2 for outbound connect (`auth.connect_profile`).
  """
  def tool_model_supports_outbound_oauth?(%{} = body) do
    auth = body["auth"] || %{}

    case ConnectProfile.from_auth(auth) do
      {:ok, p} -> p.has_oauth
      :error -> false
    end
  end

  def tool_model_supports_outbound_oauth?(_), do: false
end
