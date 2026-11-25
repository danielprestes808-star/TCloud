#!/usr/bin/env node
/**
 * Trava a dívida de `!important` no CSS do Desktop.
 *
 * `app/src/App.css` acumulou milhares de `!important`, o que torna a cascata
 * imprevisível: cada ajuste visual precisa de mais força para vencer a regra
 * anterior. Limpar tudo de uma vez quebraria telas, então este script funciona
 * como catraca — o número pode cair, nunca subir.
 *
 * Uso:
 *   node scripts/check-css-important.mjs            # compara com o baseline
 *   node scripts/check-css-important.mjs --update   # regrava o baseline
 */
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const baselinePath = join(root, "scripts", "css-important-baseline.json");
const update = process.argv.includes("--update");

const TARGETS = [
  "app/src/App.css",
  "app/src/styles/tcloud-explorer.css",
  "app/src/styles/tcloud-explorer-14.css",
  "app/src/styles/tcloud-explorer-14-base.css",
  "app/src/styles/obsidian-precision.css",
  "web/src/app/globals.css",
];

function countImportant(file) {
  const absolute = join(root, file);

  if (!existsSync(absolute)) {
    return null;
  }

  const source = readFileSync(absolute, "utf8");
  return (source.match(/!important/g) ?? []).length;
}

const current = {};

for (const file of TARGETS) {
  const count = countImportant(file);

  if (count !== null) {
    current[file] = count;
  }
}

if (update || !existsSync(baselinePath)) {
  writeFileSync(baselinePath, `${JSON.stringify(current, null, 2)}\n`);
  const total = Object.values(current).reduce((sum, value) => sum + value, 0);
  console.log(
    `[css-important] baseline gravado: ${total} ocorrências em ${Object.keys(current).length} arquivos.`,
  );
  process.exit(0);
}

const baseline = JSON.parse(readFileSync(baselinePath, "utf8"));
const regressions = [];
const improvements = [];

for (const [file, limit] of Object.entries(baseline)) {
  const count = current[file] ?? 0;

  if (count > limit) {
    regressions.push(`${relative(root, join(root, file))}: ${count} > ${limit}`);
  } else if (count < limit) {
    improvements.push(`${file}: ${count} < ${limit}`);
  }
}

if (regressions.length > 0) {
  console.error(
    "[css-important] novas ocorrências de !important foram adicionadas:\n" +
      regressions.map((item) => `  - ${item}`).join("\n") +
      "\n  Resolva a especificidade em vez de forçar a regra, ou rode --update se a mudança for intencional.",
  );
  process.exit(1);
}

if (improvements.length > 0) {
  console.log(
    `[css-important] dívida reduziu em ${improvements.length} arquivo(s). Rode --update para travar o novo patamar.`,
  );
  for (const item of improvements) {
    console.log(`  - ${item}`);
  }
  process.exit(0);
}

const total = Object.values(current).reduce((sum, value) => sum + value, 0);
console.log(`[css-important] OK — ${total} ocorrências, dentro do baseline.`);
