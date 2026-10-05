const MIDAS_MARK_PATH = "M32 9 V55 M44 12 H28 A10 10 0 0 0 28 32 H36 A10 10 0 0 1 36 52 H20"

const FAVICON_STROKE = {
  light: "#000",
  dark: "#fff",
} as const

// Browsers render the SVG favicon once and never re-evaluate its
// prefers-color-scheme styles, so a color scheme change leaves the old color
// in the tab until the next page load. Replacing the link with a data URL
// carrying the scheme-colored mark updates the icon immediately.
function applyFavicon() {
  const dark = window.matchMedia("(prefers-color-scheme: dark)").matches
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="${MIDAS_MARK_PATH}" fill="none" stroke="${dark ? FAVICON_STROKE.dark : FAVICON_STROKE.light}" stroke-width="9" stroke-linecap="round" stroke-linejoin="round"/></svg>`
  const link = document.querySelector<HTMLLinkElement>('link[rel="icon"]')!
  link.href = `data:image/svg+xml,${encodeURIComponent(svg)}`
}

export function watchFavicon() {
  const media = window.matchMedia("(prefers-color-scheme: dark)")
  applyFavicon()
  media.addEventListener("change", applyFavicon)
}
