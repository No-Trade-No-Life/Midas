import { useEffect, useState } from "react"

import { applyFavicon } from "@/lib/favicon"

export const themeStorageKey = "midas.theme"

const darkModeQuery = "(prefers-color-scheme: dark)"

// The theme toggle is binary: an explicit stored "dark"/"light" wins, and a
// first visit (or any other stored value) follows the OS scheme.
function storedDark(): boolean | null {
  try {
    const stored = window.localStorage.getItem(themeStorageKey)
    if (stored === "dark") return true
    if (stored === "light") return false
  } catch (error) {
    console.warn("Midas could not read the stored theme preference; following the system scheme.", error)
  }
  return null
}

function prefersDark() {
  return window.matchMedia(darkModeQuery).matches
}

const listeners = new Set<() => void>()
let currentDark: boolean | null = null

export function isDark(): boolean {
  if (currentDark === null) currentDark = storedDark() ?? prefersDark()
  return currentDark
}

function applyDark(dark: boolean) {
  const root = document.documentElement
  root.classList.toggle("dark", dark)
  root.style.colorScheme = dark ? "dark" : "light"
  applyFavicon(dark ? "dark" : "light")
}

function notify() {
  listeners.forEach((listener) => listener())
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function toggleTheme() {
  const dark = !isDark()
  currentDark = dark
  try {
    window.localStorage.setItem(themeStorageKey, dark ? "dark" : "light")
  } catch (error) {
    console.warn("Midas could not persist the theme preference for this session.", error)
  }
  applyDark(dark)
  notify()
}

export function startTheme() {
  applyDark(isDark())
}

function useDarkState() {
  const [dark, setDark] = useState(isDark)
  useEffect(() => subscribe(() => setDark(isDark())), [])
  return dark
}

export function useTheme() {
  return { dark: useDarkState(), toggleTheme }
}

export function useIsDark() {
  return useDarkState()
}
