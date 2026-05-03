defmodule PlasmDesktop.Appliance.ControlPlaneSecret do
  @moduledoc """
  Bootstraps `:mcp_control_plane_secret` so the desktop can authorize agent `/internal/*` calls.

  When `PLASM_MCP_CONTROL_PLANE_SECRET` is unset, **dev/test** use the same default string as OSS
  `plasm-agent-core` `DEV_PLANE_SECRET_FALLBACK` in `plasm-oss/crates/plasm-agent-core/src/control_plane_http.rs`
  (keep byte-for-byte identical). **Production** (`appliance_control_plane_dev_fallback: false`)
  loads or generates a row in `desktop_settings` and logs that the agent env must match.
  """

  require Logger

  alias PlasmDesktop.Settings

  # Must stay identical to `plasm-oss/crates/plasm-agent-core/src/control_plane_http.rs`
  # (`DEV_PLANE_SECRET_FALLBACK`).
  @dev_fallback "dev-plasm-mcp-control-plane-secret-32chars-min!!"

  @settings_key "plasm_mcp_control_plane_secret"

  @doc false
  def dev_fallback, do: @dev_fallback

  @spec bootstrap!() :: :ok
  def bootstrap! do
    secret =
      try do
        resolve!()
      rescue
        e ->
          Logger.error(
            "[plasm_desktop] control plane secret bootstrap failed: #{Exception.message(e)} — using dev fallback"
          )

          @dev_fallback
      end

    Application.put_env(:plasm_desktop, :mcp_control_plane_secret, secret)
    :ok
  end

  defp resolve! do
    cond do
      (s = env_secret_valid()) != nil ->
        s

      dev_fallback_mode?() ->
        @dev_fallback

      true ->
        resolve_prod_persisted!()
    end
  end

  defp env_secret_valid do
    case System.get_env("PLASM_MCP_CONTROL_PLANE_SECRET") do
      s when is_binary(s) ->
        t = String.trim(s)

        cond do
          t == "" ->
            nil

          String.length(t) < 16 ->
            Logger.warning(
              "[plasm_desktop] PLASM_MCP_CONTROL_PLANE_SECRET is set but shorter than 16 chars — ignoring"
            )

            nil

          true ->
            t
        end

      _ ->
        nil
    end
  end

  defp dev_fallback_mode? do
    Application.get_env(:plasm_desktop, :appliance_control_plane_dev_fallback, false) == true
  end

  defp resolve_prod_persisted! do
    db = Settings.get_all_map()
    existing = db |> Map.get(@settings_key, "") |> to_string() |> String.trim()

    cond do
      existing != "" and String.length(existing) >= 16 ->
        Logger.info("[plasm_desktop] Using persisted appliance control plane secret from desktop_settings")
        existing

      true ->
        generated =
          :crypto.strong_rand_bytes(32)
          |> Base.encode64(padding: false)

        case Settings.upsert_many(%{@settings_key => generated}) do
          :ok ->
            Logger.warning(
              "[plasm_desktop] Generated and persisted PLASM_MCP_CONTROL_PLANE_SECRET for this appliance. " <>
                "Set the **same** value in the agent environment before calling /internal/*."
            )

            generated

          {:error, reason} ->
            Logger.error(
              "[plasm_desktop] Could not persist control plane secret (#{inspect(reason)}); using dev fallback — agent must match."
            )

            @dev_fallback
        end
    end
  end
end
