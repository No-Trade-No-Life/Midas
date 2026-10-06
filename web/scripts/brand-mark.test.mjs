import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const dollarMark = 'M32 9 V55 M44 12 H28 A10 10 0 0 0 28 32 H36 A10 10 0 0 1 36 52 H20';
const favicon = readFileSync(new URL('../public/midas-mark.svg', import.meta.url), 'utf8');
const markComponent = readFileSync(new URL('../src/components/midas-mark.tsx', import.meta.url), 'utf8');
const indexHtml = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const main = readFileSync(new URL('../src/main.tsx', import.meta.url), 'utf8');
const faviconSync = readFileSync(new URL('../src/lib/favicon.ts', import.meta.url), 'utf8');
const theme = readFileSync(new URL('../src/theme.tsx', import.meta.url), 'utf8');

test('the favicon is a dollar mark with a light and dark palette', () => {
  assert.ok(favicon.includes(dollarMark));
  assert.match(favicon, /path \{ stroke: #000; \}/);
  assert.match(favicon, /@media \(prefers-color-scheme: dark\) \{\s*path \{ stroke: #fff; \}/);
});

test('the navigation rail and drawer render the dollar mark in the theme foreground color', () => {
  assert.match(markComponent, /stroke="currentColor"/);
  assert.match(markComponent, /aria-hidden="true"/);
  assert.ok(markComponent.includes(dollarMark));
  assert.match(main, /<MidasMark className="size-7 shrink-0" \/>/);
  assert.match(main, /<MidasMark className="size-9" \/>/);
  assert.ok(!main.includes('dark:invert'));
});

test('the favicon links the SVG mark', () => {
  assert.match(indexHtml, /<link rel="icon" type="image\/svg\+xml" href="\/midas-mark\.svg"\/>/);
});

test('the favicon follows the effective theme, not just the system scheme', () => {
  assert.ok(faviconSync.includes(dollarMark));
  assert.match(faviconSync, /resolveDark\(currentThemeChoice\(\)\)/);
  assert.match(faviconSync, /subscribe\(applyFavicon\)/);
  assert.match(faviconSync, /data:image\/svg\+xml/);
  assert.match(main, /watchFavicon\(\)/);
  assert.match(main, /startThemeSync\(\)/);
  assert.match(theme, /prefers-color-scheme: dark/);
});
