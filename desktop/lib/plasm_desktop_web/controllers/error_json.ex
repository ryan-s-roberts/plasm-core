defmodule PlasmDesktopWeb.ErrorJSON do
  def error(%{status: status}) do
    %{errors: %{detail: http_status(status)}}
  end

  defp http_status(:internal_server_error), do: "Internal Server Error"
  defp http_status(:not_found), do: "Not Found"
  defp http_status(code), do: "HTTP #{code}"
end
