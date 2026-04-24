// NL → one Plasm path expression (eval goals must not require chaining or prior-step values).

class PlasmPlan {
  text string @description("Single Plasm path expression for the goal.")
  reasoning string @description("Brief note.")
}

// One chat turn for TranslatePlan (eval passes a single user message; REPL appends history).
class PlanChatTurn {
  role "user" | "assistant"
  content string
}

function TranslatePlan(messages: PlanChatTurn[]) -> PlasmPlan {
  client Sonnet
  prompt #"
    {{ _.role("system") }}
    You map the user's CURRENT goal to exactly ONE Plasm path expression. The final user message in the transcript contains --- GOAL --- and optionally --- CORRECTION FEEDBACK ---; that is the goal you must satisfy now. Earlier user/assistant turns are conversation context: you may use them to interpret follow-ups, but still output one self-contained expression (explicit literals for ids and filters as required by the rules below).

    Use only the expression reference from the first user message that contains the domain/schema block (legend if present, then valid-expression rules, then per-entity examples). Do not output multiple expressions, numbered lists, or steps. If correction feedback is non-empty on the final user message, revise `text` to fix the issue.
    Output only executable Plasm syntax in `text`. Never copy hint tails, result gloss, or prose from teaching rows or TSV meaning cells.

    Bad:
    `e13{p87=e16(p69="plasm", p63="plasm"), p80=12}  ;;  => [e13]  List reviews on a pull request.`

    Good:
    `e13{p87=e16(p69="plasm", p63="plasm"), p80=12}`

    {% for m in messages %}
    {{ _.role(m.role) }}
    {{ m.content }}
    {% endfor %}

    {{ ctx.output_format }}
  "#
}
