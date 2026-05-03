defmodule PlasmUiCore.Web.McpJsonHighlight do
  @moduledoc false

  import Phoenix.HTML, only: [raw: 1, html_escape: 1, safe_to_string: 1]

  @key "mcp-jh mcp-jh-key"
  @str "mcp-jh mcp-jh-str"
  @num "mcp-jh mcp-jh-num"
  @bool "mcp-jh mcp-jh-atom"
  @null "mcp-jh mcp-jh-null"
  @punct "mcp-jh mcp-jh-punct"

  @spec safe_pretty(String.t()) :: Phoenix.HTML.safe()
  def safe_pretty(s) when is_binary(s) do
    s =
      case String.trim(s) do
        <<0xEF, 0xBB, 0xBF, rest::binary>> -> String.trim(rest)
        other -> other
      end

    case Jason.decode(s) do
      {:ok, term} ->
        term
        |> Jason.encode!(pretty: true)
        |> highlight_pretty_string()

      :error ->
        html_escape(s)
    end
  end

  @spec pretty_map(map()) :: Phoenix.HTML.safe()
  def pretty_map(m) when is_map(m) do
    m
    |> Jason.encode!(pretty: true)
    |> highlight_pretty_string()
  end

  defp highlight_pretty_string(s) when is_binary(s) do
    s
    |> String.split("\n")
    |> Enum.map(&color_line/1)
    |> Enum.intersperse("\n")
    |> :lists.flatten()
    |> raw()
  end

  defp color_line(line) do
    case Regex.run(~r/^(\s*)("(?:\\.|[^"\\])*")(\s*:\s*)(.*)$/, line) do
      [_, ind, k, col, v] ->
        v = String.trim(v)

        if v == "" do
          [h(ind), span(h(k), @key), h(col)]
        else
          [h(ind), span(h(k), @key), h(col), color_value_fragment(v)]
        end

      nil ->
        case Regex.run(~r/^(\s*)("(?:\\.|[^"\\])*")(,?)\s*$/, line) do
          [_, ind, str, com] ->
            [h(ind), span(h(str), @str), h(com)]

          nil ->
            case Regex.run(
                   ~r/^(\s*)(-?\d+(?:\.\d+)?(?:[eE][+\-]?\d+)?|true|false|null)(,?)\s*$/,
                   line
                 ) do
              [_, ind, lit, com] ->
                cl =
                  cond do
                    lit in ["true", "false"] -> @bool
                    lit == "null" -> @null
                    true -> @num
                  end

                [h(ind), span(h(lit), cl), h(com)]

              nil ->
                t = String.trim(line)

                if t != "" and String.match?(t, ~r/^[\{\}\[\]],?$/) do
                  [span(h(line), @punct)]
                else
                  [h(line)]
                end
            end
        end
    end
  end

  defp color_value_fragment(v) do
    v = String.trim(v)
    com = if String.ends_with?(v, ","), do: ",", else: ""
    v0 = String.trim_trailing(v, ",") |> String.trim()

    core =
      cond do
        String.starts_with?(v0, "\"") and String.ends_with?(v0, "\"") ->
          span(h(v0), @str)

        v0 in ["true", "false"] ->
          span(h(v0), @bool)

        v0 == "null" ->
          span(h(v0), @null)

        String.match?(v0, ~r/^-?\d/) ->
          span(h(v0), @num)

        true ->
          h(v0)
      end

    [core, h(com)]
  end

  defp span(content, class) do
    ["<span class=\"", class, "\">", content, ~S(</span>)]
  end

  defp h(bin) when is_binary(bin) do
    bin |> html_escape() |> safe_to_string()
  end
end
