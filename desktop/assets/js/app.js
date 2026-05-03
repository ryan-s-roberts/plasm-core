import "phoenix_html"
import {Socket} from "phoenix"
import {LiveSocket} from "phoenix_live_view"

const csrfToken = document.querySelector("meta[name='csrf-token']").getAttribute("content")

/** Same contract as SaaS `web/` — LiveView `push_event("mcp:copy", %{text})`. */
const Hooks = {
  McpClipboardBridge: {
    mounted() {
      this.handleEvent("mcp:copy", ({text}) => {
        if (typeof text !== "string" || text === "") return
        navigator.clipboard.writeText(text).catch(() => {})
      })
    },
  },
}

const liveSocket = new LiveSocket("/live", Socket, {
  params: {_csrf_token: csrfToken},
  hooks: Hooks,
})

liveSocket.connect()

window.liveSocket = liveSocket
