import { readFileSync } from 'node:fs';

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const main = read('src/main.tsx');
const favicon = read('src/lib/favicon.ts');
const sonner = read('src/components/ui/sonner.tsx');
const styles = read('src/styles.css');
const indexHtml = read('index.html');

// Theme: the Linkit profile owns the dark mode preference; the header control
// lives inside LinkitMyInfo and the mirrored preference is applied before the
// app module loads.
if (!indexHtml.includes('window.localStorage.getItem("linkit.theme")')) throw new Error('Theme regression: the inline script must read linkit.theme before paint.');
if (!indexHtml.includes('content="#ffffff"') || !indexHtml.includes('content="#0a0a0a"')) throw new Error('Theme regression: theme-color must cover light and dark.');
const inline = indexHtml.indexOf('linkit.theme');
const module = indexHtml.indexOf('<script type="module"');
if (inline < 0 || module < 0 || inline > module) throw new Error('Theme regression: the stored theme must be applied before the app module loads.');

if (!favicon.includes('export function applyFavicon(resolvedTheme: keyof typeof FAVICON_STROKE)') || !favicon.includes('data:image/svg+xml')) throw new Error('Theme regression: the favicon must render the theme-colored mark.');
if (favicon.includes('@/theme') || favicon.includes('subscribe(')) throw new Error('Theme regression: the favicon stays a pure apply function driven by the resolved Linkit theme.');
if (!sonner.includes('useLinkit') || !sonner.includes('theme={resolvedTheme}')) throw new Error('Theme regression: toasts must follow the resolved Linkit theme.');
if (!main.includes('applyFavicon(resolvedTheme)')) throw new Error('Theme regression: the favicon must follow the resolved Linkit theme.');
if (!styles.includes('.dark {') || !styles.includes('--background: oklch(0.145 0 0)')) throw new Error('Theme regression: dark tokens are required.');
if (!styles.includes('.skip-link')) throw new Error('Layout regression: the skip link component style is missing.');

// The header theme control now lives inside LinkitMyInfo; the app keeps no
// theme store of its own.
if (!main.includes('<LinkitMyInfo />')) throw new Error('Theme regression: the header must host LinkitMyInfo.');
for (const forbidden of ['midas.theme', 'from "@/theme"', 'toggleTheme', 'useIsDark', 'startTheme', 'ThemeSwitcher', 'themeChoiceOptions', 'themeChoiceLabel', 'appearance', 'themeLight', 'themeDark', 'themeSystem', 'MonitorIcon', 'watchFavicon', 'startThemeSync']) {
  if (main.includes(forbidden)) throw new Error(`Theme regression: ${forbidden} must be gone with the app-owned theme system.`);
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
if (!main.slice(header, mobileBar).includes('<LinkitMyInfo />')) throw new Error('Layout regression: LinkitMyInfo sits in the header.');
if (!main.slice(desktop, header).includes('visibleNavigationGroups(root)') || !main.slice(drawer, admin).includes('visibleNavigationGroups(root)')) throw new Error('Layout regression: the rail and drawer must share visibleNavigationGroups.');
if (!main.slice(mobileBar, drawer).includes('lg:hidden')) throw new Error('Layout regression: the phone tab bar is for phones only.');
console.log('Midas web layout and theme check passed');
