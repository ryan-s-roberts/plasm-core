defmodule PlasmDesktop.Repo do
  use Ecto.Repo,
    otp_app: :plasm_desktop,
    adapter: Ecto.Adapters.Postgres
end
