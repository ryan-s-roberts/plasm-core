defmodule PlasmUiCore.Trace do
  @moduledoc """
  Behaviour for trace list/detail adapters used by trace LiveViews.

  Streaming URLs remain caller-defined; this covers HTTP list/detail only.
  """

  @type session :: map()

  @callback list_traces(session, opts :: keyword()) :: {:ok, term()} | {:error, term()}
  @callback fetch_trace_detail(session, trace_id :: String.t()) ::
              {:ok, map()} | {:error, term()}
end
