#!/usr/bin/env node
/**
 * Confere se o manifesto local do updater mobile acompanha o pubspec.
 *
 * O app Android lê o manifesto publicado em TCloud-Releases, mas a cópia em
 * `mobile/updater/mobile-latest.json` é versionada junto do código. Quando ela
 * fica para trás, quem for publicar uma release copia números errados.
 *
 * Uso: node scripts/check-mobile-manifest.mjs [--strict]
 *   --strict  falha (exit 1) quando há divergência; sem a flag apenas avisa.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const strict = process.argv.includes("--strict");

function fail(message) {
  console.error(`[mobile-manifest] ${message}`);
  process.exit(1);
}

let pubspec;
try {
  pubspec = readFileSync(join(root, "mobile", "pubspec.yaml"), "utf8");
} catch {
  fail("mobile/pubspec.yaml não encontrado.");
}

const versionLine = pubspec.match(/^version:\s*(\S+)\s*$/m);

if (!versionLine) {
  fail("campo `version` ausente em mobile/pubspec.yaml.");
}

const [version, buildNumber] = versionLine[1].split("+");
const versionCode = Number.parseInt(buildNumber ?? "", 10);

if (!Number.isInteger(versionCode)) {
  fail(`versão do pubspec sem build number válido: ${versionLine[1]}`);
}

let manifest;
try {
  manifest = JSON.parse(
    readFileSync(join(root, "mobile", "updater", "mobile-latest.json"), "utf8"),
  );
} catch (error) {
  fail(`mobile/updater/mobile-latest.json inválido: ${error.message}`);
}

const problems = [];

if (manifest.version !== version) {
  problems.push(`version ${manifest.version} != pubspec ${version}`);
}

if (manifest.versionCode !== versionCode) {
  problems.push(`versionCode ${manifest.versionCode} != pubspec ${versionCode}`);
}

if (!/^[a-f0-9]{64}$/.test(String(manifest.sha256 ?? ""))) {
  problems.push("sha256 ausente ou fora do formato de 64 hexadecimais");
}

if (problems.length === 0) {
  console.log(
    `[mobile-manifest] OK — manifesto ${manifest.version} (${manifest.versionCode}) acompanha o pubspec.`,
  );
  process.exit(0);
}

const detail = problems.map((item) => `  - ${item}`).join("\n");

if (strict) {
  fail(`manifesto do updater divergente do pubspec:\n${detail}`);
}

console.warn(
  `[mobile-manifest] AVISO — manifesto do updater divergente do pubspec:\n${detail}\n` +
    "  Atualize mobile/updater/mobile-latest.json ao publicar a release (o sha256 vem do APK assinado).",
);
process.exit(0);
