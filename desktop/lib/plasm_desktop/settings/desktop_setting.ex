defmodule PlasmDesktop.Settings.DesktopSetting do
  use Ecto.Schema

  @primary_key {:key, :string, autogenerate: false}
  schema "desktop_settings" do
    field :value, :string
  end
end
