defmodule PlasmDesktopWeb.CoreComponents do
  @moduledoc """
  Shared UI primitives for the appliance (aligned with `plasm_ui_core.css` classes).
  """
  use Phoenix.Component

  @doc """
  Flash messaging (`flash-group` from Phoenix generators, styled for the appliance shell).
  """
  attr :flash, :map, required: true
  attr :id, :string, default: "flash-group"

  def flash_group(assigns) do
    ~H"""
    <div id={@id} class="plasm-flash-stack" role="status">
      <.flash kind={:info} flash={@flash} />
      <.flash kind={:error} flash={@flash} />
    </div>
    """
  end

  attr :id, :string, default: nil
  attr :flash, :map, required: true
  attr :kind, :atom, required: true

  def flash(assigns) do
    msg = Phoenix.Flash.get(assigns.flash, assigns.kind)

    assigns =
      assigns
      |> assign(:msg, msg)
      |> assign(:flash_id, assigns[:id] || "flash-#{assigns.kind}")

    ~H"""
    <div
      :if={is_binary(@msg) and @msg != ""}
      id={@flash_id}
      class={flash_class(@kind)}
      role="alert"
    >
      <p class="m-0">{@msg}</p>
    </div>
    """
  end

  defp flash_class(:info), do: "plasm-flash plasm-flash-info"
  defp flash_class(:error), do: "plasm-flash plasm-flash-error"

  attr :eyebrow, :string, default: nil
  attr :title, :string, required: true
  attr :subtitle, :string, default: nil
  slot :actions

  def page_header(assigns) do
    ~H"""
    <header class="plasm-page-header">
      <div class="plasm-page-title-block">
        <p :if={@eyebrow} class="plasm-eyebrow">{@eyebrow}</p>
        <h1>{@title}</h1>
        <p :if={@subtitle} class="plasm-page-subtitle">{@subtitle}</p>
      </div>
      <div class="plasm-page-actions">{render_slot(@actions)}</div>
    </header>
    """
  end

  @doc """
  Compact instrument readout for the hub page (`YourMcpSnapshot`).
  """
  attr :snap, :map, required: true

  def station_status_strip(assigns) do
    snap = assigns.snap
    policy = snap.policy
    reg_ok = snap.registry_ok
    cp_ok = policy.control_plane_ok
    n_apis = policy.selected_ids |> MapSet.size()
    n_keys = length(snap.api_keys || [])
    http = uri_hint(snap.http_base)
    mcp = uri_hint(snap.mcp_public_base)
    last_tid = first_trace_id(snap.traces)

    assigns =
      assigns
      |> assign(:reg_ok, reg_ok)
      |> assign(:cp_ok, cp_ok)
      |> assign(:n_apis, n_apis)
      |> assign(:n_keys, n_keys)
      |> assign(:http, http)
      |> assign(:mcp, mcp)
      |> assign(:last_tid, last_tid)
      |> assign(:policy_name, policy.policy_name || "MCP policy")

    ~H"""
    <section class="plasm-station-strip" aria-label="Agent and policy status">
      <div class="plasm-station-strip-chips">
        <span class={chip_class(@reg_ok)}>
          <%= if @reg_ok do %>
            Registry live
          <% else %>
            Registry blocked
          <% end %>
        </span>
        <span class={chip_class(@cp_ok)}>
          <%= if @cp_ok do %>
            Control plane ready
          <% else %>
            Control plane idle
          <% end %>
        </span>
        <span class="plasm-station-chip plasm-station-chip--neutral">
          {@policy_name}
        </span>
      </div>
      <dl class="plasm-station-strip-metrics">
        <div>
          <dt>Linked APIs</dt>
          <dd>{@n_apis}</dd>
        </div>
        <div>
          <dt>Transport keys</dt>
          <dd>{@n_keys}</dd>
        </div>
        <div class="plasm-station-strip-metric-wide">
          <dt>Discovery HTTP</dt>
          <dd><code class="plasm-station-mono">{@http}</code></dd>
        </div>
        <div class="plasm-station-strip-metric-wide">
          <dt>Streamable MCP</dt>
          <dd><code class="plasm-station-mono">{@mcp}</code></dd>
        </div>
      </dl>
      <%= if is_binary(@last_tid) and @last_tid != "" do %>
        <div class="plasm-station-strip-footer">
          <span class="plasm-station-strip-footer-label">Latest trace</span>
          <a class="plasm-station-trace-link plasm-station-mono" href={"/traces/#{URI.encode(@last_tid)}"}>
            {String.slice(@last_tid, 0, 14)}…
          </a>
          <a class="plasm-button plasm-button-ghost plasm-station-strip-footer-all" href="/traces">
            All traces
          </a>
        </div>
      <% end %>
    </section>
    """
  end

  defp chip_class(true), do: "plasm-station-chip plasm-station-chip--ok"
  defp chip_class(false), do: "plasm-station-chip plasm-station-chip--danger"

  defp uri_hint(base) when is_binary(base) do
    t = String.trim(base)

    cond do
      t == "" ->
        "—"

      String.length(t) <= 52 ->
        t

      true ->
        String.slice(t, 0, 49) <> "…"
    end
  end

  defp uri_hint(_), do: "—"

  defp first_trace_id(traces) when is_list(traces) do
    case List.first(traces) do
      row when is_map(row) -> row["trace_id"] || row[:trace_id] || ""
      _ -> ""
    end
  end

  defp first_trace_id(_), do: ""

  attr :tone, :atom, default: :default
  attr :id, :string, default: nil
  slot :inner_block, required: true

  def panel(assigns) do
    extra =
      case assigns.tone do
        :success -> " plasm-panel-success"
        :warning -> " plasm-panel-warning"
        _ -> ""
      end

    assigns = assign(assigns, :extra, extra)

    ~H"""
    <section id={@id} class={"plasm-panel#{@extra}"}>
      {render_slot(@inner_block)}
    </section>
    """
  end

  attr :title, :string, required: true
  attr :description, :string, default: nil
  slot :actions

  def section_header(assigns) do
    ~H"""
    <div class="plasm-section-header">
      <div>
        <h2>{@title}</h2>
        <p :if={@description}>{@description}</p>
      </div>
      <div class="plasm-section-actions">{render_slot(@actions)}</div>
    </div>
    """
  end

  attr :type, :string, default: "button"
  attr :variant, :atom, default: :secondary
  attr :navigate, :string, default: nil
  attr :class, :string, default: nil
  attr :disabled, :boolean, default: false
  attr :rest, :global
  slot :inner_block, required: true

  def button(assigns) do
    v =
      case assigns.variant do
        :primary -> "plasm-button-primary"
        :ghost -> "plasm-button-ghost"
        :quiet -> "plasm-button-ghost"
        _ -> "plasm-button-secondary"
      end

    assigns = assign(assigns, :v, v)

    ~H"""
    <%= if @navigate do %>
      <.link navigate={@navigate} class={["plasm-button", @v, @class]} {@rest}>
        {render_slot(@inner_block)}
      </.link>
    <% else %>
      <button
        type={@type}
        class={["plasm-button", @v, @class]}
        disabled={@disabled}
        {@rest}
      >
        {render_slot(@inner_block)}
      </button>
    <% end %>
    """
  end

  attr :tone, :atom, default: :default
  slot :inner_block, required: true

  def notice(assigns) do
    cls =
      case assigns.tone do
        :warning -> "plasm-notice plasm-notice-warning"
        :danger -> "plasm-notice plasm-notice-danger"
        :success -> "plasm-notice plasm-notice-success"
        _ -> "plasm-notice"
      end

    assigns = assign(assigns, :cls, cls)

    ~H"""
    <div class={@cls}>{render_slot(@inner_block)}</div>
    """
  end

  attr :id, :string, default: nil
  attr :type, :string, default: "text"
  attr :name, :string, default: nil
  attr :value, :any, default: nil
  attr :placeholder, :string, default: nil
  attr :autocomplete, :string, default: nil
  attr :disabled, :boolean, default: false
  attr :readonly, :boolean, default: false
  attr :required, :boolean, default: false
  attr :checked, :boolean, default: false
  attr :class, :string, default: nil
  attr :rest, :global

  def control_input(assigns) do
    ~H"""
    <input
      type={@type}
      id={@id}
      name={@name}
      value={@value}
      placeholder={@placeholder}
      autocomplete={@autocomplete}
      disabled={@disabled}
      readonly={@readonly}
      required={@required}
      checked={@checked}
      class={[@class || "plasm-input"]}
      {@rest}
    />
    """
  end

  attr :entry_id, :string, required: true
  attr :label, :string, default: nil
  attr :checked, :boolean, default: false

  def catalog_pick_row(assigns) do
    ~H"""
    <div
      class={["plasm-catalog-row", @checked && "is-selected"]}
      phx-click="toggle_entry"
      phx-value-id={@entry_id}
      role="button"
      tabindex="0"
    >
      <input type="checkbox" checked={@checked} tabindex="-1" disabled />
      <div class="plasm-catalog-copy">
        <strong>{@label || @entry_id}</strong>
        <span>{@entry_id}</span>
      </div>
    </div>
    """
  end

  attr :label, :string, required: true
  attr :value, :any, required: true

  def metric(assigns) do
    ~H"""
    <div class="plasm-metric">
      <p>{@label}</p>
      <strong>{@value}</strong>
    </div>
    """
  end

  attr :tone, :atom, default: :muted
  slot :inner_block, required: true

  def status_badge(assigns) do
    cls =
      case assigns.tone do
        :success -> "plasm-status plasm-status-success"
        :warning -> "plasm-status plasm-status-warning"
        _ -> "plasm-status plasm-status-muted"
      end

    assigns = assign(assigns, :cls, cls)

    ~H"""
    <span class={@cls}>{render_slot(@inner_block)}</span>
    """
  end

  attr :label, :string, required: true
  attr :value, :any, required: true
  attr :muted, :boolean, default: false

  def value_row(assigns) do
    ~H"""
    <div class="plasm-value-row">
      <span>{@label}</span>
      <code class={if(@muted, do: "is-muted")}>{@value}</code>
    </div>
    """
  end

  attr :traces, :list, required: true

  def trace_table(assigns) do
    assigns = assign(assigns, :sorted, sort_traces(assigns.traces || []))

    ~H"""
    <div class="plasm-traces-table-wrap">
      <table class="plasm-traces-table" id="appliance-trace-table">
        <thead>
          <tr>
            <th>Status</th>
            <th>Trace</th>
            <th>Started</th>
            <th>Session span</th>
            <th>Σ processing</th>
            <th>Token mix</th>
            <th>Code</th>
            <th>Net</th>
          </tr>
        </thead>
        <tbody>
          <%= for t <- @sorted do %>
            <% tid = t["trace_id"] || "" %>
            <tr>
              <td>
                <.status_badge tone={if(t["status"] == "live", do: :success, else: :muted)}>
                  {t["status"] || "—"}
                </.status_badge>
              </td>
              <td>
                <.link class="font-mono text-xs" navigate={"/traces/#{URI.encode(tid)}"}>
                  {String.slice(tid, 0, 13)}…
                </.link>
              </td>
              <td>
                <.trace_time ms={t["started_at_ms"]} />
              </td>
              <td class="tabular-nums">{session_span_cell(t)}</td>
              <td class="tabular-nums">{processing_cell(t)}</td>
              <td>
                <.token_strip totals={t["totals"]} />
              </td>
              <td class="tabular-nums">{code_cell(t)}</td>
              <td class="tabular-nums">{net_cell(t)}</td>
            </tr>
          <% end %>
        </tbody>
      </table>
    </div>
    """
  end

  attr :ms, :any, required: true

  def trace_time(assigns) do
    ms = normalize_ms(assigns.ms)

    assigns =
      assign(assigns,
        ms: ms,
        label: if(is_integer(ms), do: format_dt(ms), else: "—")
      )

    ~H"""
    <span class="tabular-nums">{@label}</span>
    """
  end

  attr :totals, :map, default: %{}

  def token_strip(assigns) do
    t = assigns.totals || %{}
    d = tot_int(t, "domain_prompt_chars")
    i = tot_int(t, "plasm_invocation_chars")
    r = tot_int(t, "plasm_response_chars")
    rr = tot_int(t, "mcp_resource_read_chars")
    sum = d + i + r + rr

    assigns =
      assign(assigns,
        d: d,
        i: i,
        r: r,
        rr: rr,
        label: "Σ #{sum} chars (domain #{d} · invoke #{i} · resp #{r} · resources #{rr})"
      )

    ~H"""
    <div class="plasm-token-strip" title={@label}>
      <span style={"flex-grow: #{max(@d, 1)};background:oklch(0.62 0.12 252 / 0.85)"} />
      <span style={"flex-grow: #{max(@i, 1)};background:oklch(0.72 0.12 70 / 0.85)"} />
      <span style={"flex-grow: #{max(@r, 1)};background:oklch(0.72 0.12 150 / 0.85)"} />
      <span style={"flex-grow: #{max(@rr, 1)};background:oklch(0.72 0.11 294 / 0.85)"} />
    </div>
    <span class="tabular-nums text-xs" style="display:block;margin-top:0.25rem;color:var(--plasm-muted)">
      {@label}
    </span>
    """
  end

  defp sort_traces(traces) when is_list(traces) do
    Enum.sort_by(traces, fn t -> normalize_ms(t["started_at_ms"]) || 0 end, :desc)
  end

  defp session_span_cell(t) do
    started = normalize_ms(t["started_at_ms"])
    ended = normalize_ms(t["ended_at_ms"])

    case {started, ended} do
      {s, e} when is_integer(s) and is_integer(e) and e >= s ->
        format_short_ms(e - s)

      _ ->
        "—"
    end
  end

  defp processing_cell(t) do
    case tot_int(t["totals"] || %{}, "total_duration_ms") do
      n when is_integer(n) and n >= 0 -> format_short_ms(n)
      _ -> "—"
    end
  end

  defp format_short_ms(ms) when is_integer(ms) and ms >= 0 do
    cond do
      ms >= 86_400_000 -> "#{div(ms, 86_400_000)}d+"
      ms >= 3_600_000 -> "#{div(ms, 3_600_000)}h #{div(rem(ms, 3_600_000), 60_000)}m"
      ms >= 60_000 -> "#{div(ms, 60_000)}m"
      true -> "#{ms} ms"
    end
  end

  defp code_cell(t) do
    tot = t["totals"] || %{}
    e = tot_int(tot, "code_plans_evaluated")
    x = tot_int(tot, "code_plans_executed")
    "#{e} eval / #{x} exec"
  end

  defp net_cell(t) do
    tot = t["totals"] || %{}
    to_string(Map.get(tot, "network_requests", "—"))
  end

  defp normalize_ms(n) when is_integer(n), do: n

  defp normalize_ms(b) when is_binary(b) do
    case Integer.parse(b) do
      {i, _} -> i
      :error -> nil
    end
  end

  defp normalize_ms(_), do: nil

  defp format_dt(ms) when is_integer(ms) do
    case DateTime.from_unix(ms, :millisecond) do
      {:ok, dt} -> Calendar.strftime(dt, "%Y-%m-%d %H:%M:%S UTC")
      _ -> "—"
    end
  end

  defp tot_int(nil, _), do: 0

  defp tot_int(m, k) when is_map(m) do
    case Map.get(m, k) do
      n when is_integer(n) -> n
      b when is_binary(b) ->
        case Integer.parse(b) do
          {i, _} -> i
          :error -> 0
        end

      _ ->
        0
    end
  end
end
