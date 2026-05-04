/**
 * Brand registry icons: CSP blocks inline `onerror`; swap to monogram fallback via listeners.
 */
export const McpBrandIcon = {
  mounted() {
    this._wire()
  },

  _wire() {
    const img = this.el.querySelector("img.brand-icon-img")
    const fallback = this.el.querySelector("[data-mcp-icon-fallback]")
    if (!img || !fallback) return

    const showFallback = () => {
      if (!img.isConnected) return
      img.remove()
      fallback.classList.remove("hidden")
      fallback.classList.add("flex")
    }

    if (img.complete && img.naturalWidth === 0) {
      showFallback()
      return
    }

    img.addEventListener("error", showFallback, {once: true})
    img.addEventListener(
      "load",
      () => {
        if (img.naturalWidth === 0) showFallback()
      },
      {once: true}
    )
  },
}
