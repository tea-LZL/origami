#!/usr/bin/env node
// WCAG AA contrast gate for theme tokens. Parses ui/src/app.css theme blocks
// and asserts contrast for the token pairs the spec pins:
//   --fg/--bg, --fg/--bg-raised, --fg-muted/--bg,
//   --accent-fg/--accent, --danger-fg/--danger, --tag-fg/--tag-* (each hue)
// Body text pairs need 4.5:1; large/UI text pairs need 3:1.
// Usage: node ui/scripts/check-contrast.mjs

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const cssPath = join(dirname(fileURLToPath(import.meta.url)), "..", "src", "app.css");
const css = readFileSync(cssPath, "utf8");

function parseBlocks(source) {
  const blocks = {};
  const stripped = source.replace(/\/\*[\s\S]*?\*\//g, "");
  const re = /(:root\[data-theme="[^"]+"\]|:root)\s*\{([^}]*)\}/g;
  let match;
  while ((match = re.exec(stripped)) !== null) {
    const name = match[1] === ":root" ? "light" : match[1].replace(/:root\[data-theme="([^"]+)"\]/, "$1");
    const vars = {};
    for (const line of match[2].split(";")) {
      const kv = line.match(/^\s*(--[\w-]+)\s*:\s*(#\w{3,8})\s*$/);
      if (kv) vars[kv[1]] = kv[2];
    }
    blocks[name] = { ...blocks[name], ...vars };
  }
  return blocks;
}

function hexToRgb(hex) {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? [...h].map((c) => c + c).join("") : h;
  return [
    parseInt(full.slice(0, 2), 16),
    parseInt(full.slice(2, 4), 16),
    parseInt(full.slice(4, 6), 16),
  ];
}

function relativeLuminance(hex) {
  const [r, g, b] = hexToRgb(hex).map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(fg, bg) {
  const a = relativeLuminance(fg);
  const b = relativeLuminance(bg);
  const [hi, lo] = a > b ? [a, b] : [b, a];
  return (hi + 0.05) / (lo + 0.05);
}

const blocks = parseBlocks(css);
const failures = [];

function check(theme, label, fgVar, bgVar, min) {
  const block = blocks[theme];
  const fg = block?.[fgVar];
  const bg = block?.[bgVar];
  if (!fg || !bg) {
    failures.push(`${theme}: missing token for ${label} (${fgVar}/${bgVar})`);
    return;
  }
  const ratio = contrast(fg, bg);
  if (ratio < min) {
    failures.push(`${theme}: ${label} ${fgVar}/${bgVar} = ${ratio.toFixed(2)}:1 (needs ${min}:1)`);
  }
}

for (const theme of Object.keys(blocks)) {
  check(theme, "body on bg", "--fg", "--bg", 4.5);
  check(theme, "body on raised", "--fg", "--bg-raised", 4.5);
  check(theme, "muted on bg", "--fg-muted", "--bg", 4.5);
  check(theme, "accent label", "--accent-fg", "--accent", 3);
  check(theme, "danger label", "--danger-fg", "--danger", 3);
  for (const hue of [
    "blue", "violet", "indigo", "teal", "cyan", "green",
    "lime", "amber", "orange", "rose", "plum", "slate",
  ]) {
    check(theme, `tag ${hue}`, "--tag-fg", `--tag-${hue}`, 3);
  }
}

if (failures.length > 0) {
  console.error("contrast gate FAILED:");
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}
console.log(`contrast gate ok (${Object.keys(blocks).length} themes)`);
