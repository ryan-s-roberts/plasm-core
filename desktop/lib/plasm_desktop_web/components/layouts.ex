defmodule PlasmDesktopWeb.Layouts do
  use PlasmDesktopWeb, :html

  use Phoenix.VerifiedRoutes,
    endpoint: PlasmDesktopWeb.Endpoint,
    router: PlasmDesktopWeb.Router,
    statics: PlasmDesktopWeb.static_paths()

  embed_templates "layouts/*"
end
