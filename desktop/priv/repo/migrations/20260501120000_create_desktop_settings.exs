defmodule PlasmDesktop.Repo.Migrations.CreateDesktopSettings do
  use Ecto.Migration

  def change do
    create table(:desktop_settings, primary_key: false) do
      add :key, :string, primary_key: true, null: false
      add :value, :text
    end
  end
end
