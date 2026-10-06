import { useEffect, useState } from "react"

export type ThemeChoice = "light" | "dark" | "system"

export const themeStorageKey = "midas.theme"

const darkModeQuery = "(prefers-color-scheme: dark)"
const listeners = new Set<() => void>()

let currentChoice: ThemeChoice | null = null
let watchingSystem = false

function prefersDark() {
  return window.matchMedia(darkModeQuery).matches
}

export function resolveDark(choice: ThemeChoice) {
  return choice === "dark" || (choice === "system" && prefersDark())
}

function readChoice(): ThemeChoice {
  try {
    const stored = window.localStorage.getItem(themeStorageKey)
    if (stored === "light" || stored === "dark" || stored === "system") return stored
  } catch (error) {
    console.warn("Midas could not read the stored theme preference; following the system scheme.", error)
  }
  return "system"
}

export function currentThemeChoice(): ThemeChoice {
  currentChoice ??= readChoice()
  return currentChoice
}

function applyChoice(choice: ThemeChoice) {
  const dark = resolveDark(choice)
  const root = document.documentElement
  root.classList.toggle("dark", dark)
  root.style.colorScheme = dark ? "dark" : "light"
}

function notify() {
  listeners.forEach((listener) => listener())
}

export function setThemeChoice(choice: ThemeChoice) {
  currentChoice = choice
  try {
    window.localStorage.setItem(themeStorageKey, choice)
  } catch (error) {
    console.warn("Midas could not persist the theme preference for this session.", error)
  }
  applyChoice(choice)
  notify()
}

export function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function startThemeSync() {
  applyChoice(currentThemeChoice())
  if (watchingSystem) return
  watchingSystem = true
  window.matchMedia(darkModeQuery).addEventListener("change", () => {
    if (currentThemeChoice() !== "system") return
    applyChoice("system")
    notify()
  })
}

export function useTheme() {
  const [choice, setChoice] = useState(currentThemeChoice)
  useEffect(() => {
    applyChoice(currentThemeChoice())
    return subscribe(() => setChoice(currentThemeChoice()))
  }, [])
  return { choice, setChoice: setThemeChoice }
}

export function useIsDark() {
  const [dark, setDark] = useState(() => resolveDark(currentThemeChoice()))
  useEffect(() => subscribe(() => setDark(resolveDark(currentThemeChoice()))), [])
  return dark
}
