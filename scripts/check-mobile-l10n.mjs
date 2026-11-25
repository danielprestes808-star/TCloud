#!/usr/bin/env node
/**
 * Garante paridade de chaves entre os arquivos ARB do Mobile.
 *
 * O `flutter gen-l10n` gera uma classe por locale. Se uma chave existir só no
 * template (`app_pt.arb`), os outros locales caem silenciosamente no texto em
 * português — o app "funciona", mas fica meio traduzido. Este script falha
 * quando os conjuntos de chaves divergem.
 *
 * Uso: node scripts/check-mobile-l10n.mjs
 */
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const arbDir = join(root, "mobile", "lib", "l10n");
const TEMPLATE = "app_pt.arb";

function keysOf(file) {
  const data = JSON.parse(readFileSync(join(arbDir, file), "utf8"));
  return new Set(
    Object.keys(data).filter((key) => !key.startsWith("@")),
  );
}

let files;

try {
  files = readdirSync(arbDir).filter((name) => name.endsWith(".arb"));
} catch {
  console.error(`[mobile-l10n] diretório não encontrado: ${arbDir}`);
  process.exit(1);
}

if (!files.includes(TEMPLATE)) {
  console.error(`[mobile-l10n] template ${TEMPLATE} ausente em ${arbDir}.`);
  process.exit(1);
}

const templateKeys = keysOf(TEMPLATE);
const problems = [];

for (const file of files) {
  if (file === TEMPLATE) {
    continue;
  }

  const keys = keysOf(file);
  const missing = [...templateKeys].filter((key) => !keys.has(key));
  const extra = [...keys].filter((key) => !templateKeys.has(key));

  if (missing.length > 0) {
    problems.push(`${file}: faltam ${missing.length} chave(s) — ${missing.slice(0, 6).join(", ")}${missing.length > 6 ? "…" : ""}`);
  }

  if (extra.length > 0) {
    problems.push(`${file}: ${extra.length} chave(s) que não existem no template — ${extra.slice(0, 6).join(", ")}${extra.length > 6 ? "…" : ""}`);
  }
}

if (problems.length > 0) {
  console.error(
    "[mobile-l10n] arquivos ARB divergentes:\n" +
      problems.map((item) => `  - ${item}`).join("\n"),
  );
  process.exit(1);
}

console.log(
  `[mobile-l10n] OK — ${templateKeys.size} chaves idênticas em ${files.length} locales.`,
);
