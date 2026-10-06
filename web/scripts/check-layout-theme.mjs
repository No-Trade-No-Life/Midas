import { readFileSync } from 'node:fs';

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const main = read('src/main.tsx');
const theme = read('src/theme.tsx');
const favicon = read('src/lib/favicon.ts');
const sonner = read('src/components/ui/sonner.tsx');
const styles = read('src/styles.css');
const indexHtml = read('index.html');

// Theme: a binary dark/light toggle in the header, persisted as one stored
// value; a first visit (or any legacy stored value) follows the OS scheme.
for (const expected of [
  'export const themeStorageKey = "midas.theme"',
  'const darkModeQuery = "(prefers-color-scheme: dark)"',
  'if (stored === "dark") return true',
  'if (stored === "light") return false',
  'export function isDark(): boolean',
  'root.classList.toggle("dark", dark)',
  'root.style.colorScheme = dark ? "dark" : "light"',
  'applyFavicon(dark ? "dark" : "light")',
  'export function toggleTheme()',
  'export function startTheme()',
  'export function useTheme()',
  'export function useIsDark()',
]) {
  if (!theme.includes(expected)) throw new Error(`Theme regression: missing ${expected}`);
}
if (theme.includes('"system"') || theme.includes('ThemeChoice') || theme.includes('setThemeChoice') || theme.includes('startThemeSync')) throw new Error('Theme regression: the toggle must stay binary dark/light.');

if (!indexHtml.includes('window.localStorage.getItem("midas.theme")')) throw new Error('Theme regression: the inline script must read midas.theme before paint.');
if (!indexHtml.includes('content="#ffffff"') || !indexHtml.includes('content="#0a0a0a"')) throw new Error('Theme regression: theme-color must cover light and dark.');
const inline = indexHtml.indexOf('midas.theme');
const module = indexHtml.indexOf('<script type="module"');
if (inline < 0 || module < 0 || inline > module) throw new Error('Theme regression: the stored theme must be applied before the app module loads.');

if (!favicon.includes('export function applyFavicon(resolvedTheme: keyof typeof FAVICON_STROKE)') || !favicon.includes('data:image/svg+xml')) throw new Error('Theme regression: the favicon must render the theme-colored mark.');
if (favicon.includes('@/theme') || favicon.includes('subscribe(')) throw new Error('Theme regression: the favicon stays a pure apply function driven by the theme module.');
if (!sonner.includes('useIsDark') || !sonner.includes('theme={dark ? "dark" : "light"}')) throw new Error('Theme regression: toasts must follow the effective theme.');
if (!styles.includes('.dark {') || !styles.includes('--background: oklch(0.145 0 0)')) throw new Error('Theme regression: dark tokens are required.');
if (!styles.includes('.skip-link')) throw new Error('Layout regression: the skip link component style is missing.');

// The single header toggle replaces the dropdown switcher and the Settings card.
if (!main.includes('aria-label={t("theme")}') || !main.includes('onClick={toggleTheme}') || !main.includes('{dark ? <SunIcon /> : <MoonIcon />}') || !main.includes('size="icon-sm"') || !main.includes('variant="ghost"')) throw new Error('Theme regression: the header toggle button is missing.');
if (!main.includes('theme: "Theme"') || !main.includes('dark: "Dark"')) throw new Error('Theme regression: the theme toggle copy keys (theme/dark) are missing.');
for (const forbidden of ['ThemeSwitcher', 'themeChoiceOptions', 'themeChoiceLabel', 'appearance', 'themeLight', 'themeDark', 'themeSystem', 'MonitorIcon', 'watchFavicon', 'startThemeSync']) {
  if (main.includes(forbidden)) throw new Error(`Theme regression: ${forbidden} must be gone with the dropdown switcher.`);
}

// Shell: rail + header + phone tab bar, and the collapse toggle lives in the header.
for (const expected of [
  'function DesktopNavigation(',
  'function AppHeader(',
  'function MobileNavigationBar(',
  'bg-muted/30',
  'lg:grid-cols-[3rem_minmax(0,1fr)]',
  'lg:grid-cols-[16rem_minmax(0,1fr)]',
  'data-collapsed={collapsed}',
  'midas.navigation.collapsed.v1',
  'window.localStorage.setItem(navigationCollapsedStorageKey',
  '<h1 className="truncate text-sm font-medium">',
  'compact ? "h-8" : "min-h-11"',
  'mobileNavigationItems',
]) {
  if (!main.includes(expected)) throw new Error(`Layout regression: missing ${expected}`);
}

const desktop = main.indexOf('function DesktopNavigation(');
const header = main.indexOf('function AppHeader(');
const mobileBar = main.indexOf('function MobileNavigationBar(');
const drawer = main.indexOf('function NavigationDrawer(');
const admin = main.indexOf('function AdminRoute(');
if (desktop < 0 || header < 0 || mobileBar < 0 || drawer < 0 || admin < 0 || desktop >= header || header >= mobileBar || drawer >= admin) {
  throw new Error('Layout regression: shell components must stay in order: rail, header, phone tab bar, drawer.');
}
if (main.slice(desktop, header).includes('onToggleNavigation')) throw new Error('Layout regression: the navigation toggle belongs in AppHeader, not the rail.');
if (!main.slice(header, mobileBar).includes('onClick={onToggleNavigation}') || !main.slice(header, mobileBar).includes('className="hidden lg:inline-flex"')) throw new Error('Layout regression: AppHeader must host the desktop navigation toggle.');
if (!main.slice(header, mobileBar).includes('aria-label={t("theme")}') || !main.slice(header, mobileBar).includes('LinkitMyInfo')) throw new Error('Layout regression: the theme toggle sits in the header next to LinkitMyInfo.');
if (!main.slice(desktop, header).includes('visibleNavigationGroups(root)') || !main.slice(drawer, admin).includes('visibleNavigationGroups(root)')) throw new Error('Layout regression: the rail and drawer must share visibleNavigationGroups.');
if (!main.slice(mobileBar, drawer).includes('lg:hidden')) throw new Error('Layout regression: the phone tab bar is for phones only.');
console.log('Midas web layout and theme check passed');
