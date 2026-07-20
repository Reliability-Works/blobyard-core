import { existsSync, readFileSync } from "node:fs";
import { basename, extname, relative, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const repositoryRoot = resolve(import.meta.dirname, "..");
const explicitFiles = parseArguments(process.argv.slice(2));
const ignoredFragments = [
  "/node_modules/",
  "/.next/",
  "/.turbo/",
  "/coverage/",
  "/dist/",
  "/out/",
  "/target/",
  "/playwright-report/",
  "/test-results/",
  "/report/",
  "/scripts/tests/fixtures/no-npm/",
];
const actions = new Map([
  [
    "npm",
    new Set([
      "adduser",
      "audit",
      "build",
      "ci",
      "config",
      "create",
      "exec",
      "i",
      "init",
      "install",
      "link",
      "login",
      "logout",
      "pack",
      "publish",
      "run",
      "start",
      "test",
      "token",
      "uninstall",
      "update",
      "version",
      "view",
      "whoami",
    ]),
  ],
  ["npx", null],
  [
    "yarn",
    new Set([
      "add",
      "build",
      "create",
      "dlx",
      "install",
      "lint",
      "publish",
      "run",
      "test",
      "why",
      "workspace",
      "workspaces",
    ]),
  ],
  ["bun", new Set(["add", "build", "create", "install", "publish", "run", "test", "x"])],
]);

function parseArguments(args) {
  const files = [];

  for (let index = 0; index < args.length; index += 1) {
    if (args[index] !== "--scan-file" || !args[index + 1]) {
      throw new Error(`unknown argument: ${args[index] ?? ""}`);
    }
    files.push(resolve(args[index + 1]));
    index += 1;
  }

  return files;
}

function repositoryFiles() {
  const result = spawnSync(
    "git",
    ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
    { cwd: repositoryRoot, encoding: "buffer" },
  );

  if (result.status !== 0) {
    process.stderr.write(result.stderr);
    throw new Error("unable to enumerate repository files");
  }

  return result.stdout
    .toString("utf8")
    .split("\0")
    .filter(Boolean)
    .map((file) => resolve(repositoryRoot, file));
}

function normalizedRelativePath(file) {
  return `/${relative(repositoryRoot, file).replaceAll("\\", "/")}`;
}

function shouldIgnore(file) {
  const normalized = normalizedRelativePath(file);
  return ignoredFragments.some((fragment) => normalized.includes(fragment));
}

function splitShellSegments(line) {
  const segments = [];
  let current = "";
  let quote = "";

  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    const next = line[index + 1] ?? "";

    if ((character === '"' || character === "'") && (!quote || quote === character)) {
      quote = quote ? "" : character;
      current += character;
      continue;
    }
    if (!quote && (character === ";" || character === "|" || character === "&")) {
      segments.push(current);
      current = "";
      if (next === character) index += 1;
      continue;
    }
    current += character;
  }

  segments.push(current);
  return segments;
}

function shellWords(segment) {
  return (
    segment.match(/"[^"]*"|'[^']*'|[^\s]+/gu)?.map((word) => word.replace(/^['"]|['"]$/gu, "")) ??
    []
  );
}

function stripCommandPreamble(words) {
  const result = [...words];
  const controlWords = new Set(["$", "!", "do", "if", "then", "time", "until", "while"]);
  const wrappers = new Set(["command", "corepack", "exec", "sudo", "xargs"]);

  while (result.length > 0) {
    const word = result[0].replace(/^[-+]/u, "");
    if (controlWords.has(result[0]) || word === "run") {
      result.shift();
      continue;
    }
    if (/^[A-Za-z_][A-Za-z0-9_]*=/u.test(result[0])) {
      result.shift();
      continue;
    }
    if (result[0] === "env") {
      result.shift();
      while (result[0]?.startsWith("-") || /^[A-Za-z_][A-Za-z0-9_]*=/u.test(result[0] ?? "")) {
        const option = result.shift();
        if ((option === "-u" || option === "--unset") && result.length > 0) result.shift();
      }
      continue;
    }
    if (wrappers.has(result[0])) {
      result.shift();
      while (result[0]?.startsWith("-")) result.shift();
      continue;
    }
    break;
  }

  return result;
}

function commandViolation(line) {
  const prepared = line
    .replace(/^\s*(?:-\s*)?run\s*:\s*/u, "")
    .replace(/^\s*RUN\s+/iu, "")
    .replace(/^\s*-\s+/u, "")
    .trim();

  for (const segment of splitShellSegments(prepared)) {
    const words = stripCommandPreamble(shellWords(segment));
    const command = basename(words[0] ?? "")
      .replace(/\.cmd$/iu, "")
      .toLowerCase();
    const allowedActions = actions.get(command);

    if (!actions.has(command)) continue;
    const nextWord = (words[1] ?? "").replace(/^['"]|['"]$/gu, "").toLowerCase();
    if (
      nextWord === "" ||
      nextWord.startsWith("-") ||
      allowedActions === null ||
      allowedActions.has(nextWord)
    ) {
      return command;
    }
  }

  return null;
}

function markdownViolations(lines) {
  const violations = [];
  let fence = "";

  lines.forEach((line, index) => {
    const opening = line.match(/^\s*(```+|~~~+)/u);
    if (opening) {
      fence = fence ? "" : opening[1][0];
      return;
    }
    if (!fence && !/^ {4}\S/u.test(line)) return;
    const tool = commandViolation(line);
    if (tool) violations.push({ line: index + 1, tool });
  });

  return violations;
}

function packageScriptViolations(content) {
  let parsed;
  try {
    parsed = JSON.parse(content);
  } catch {
    return [];
  }

  return Object.entries(parsed.scripts ?? {}).flatMap(([name, command]) => {
    if (typeof command !== "string") return [];
    const tool = commandViolation(command);
    return tool ? [{ line: `scripts.${name}`, tool }] : [];
  });
}

function sourceInvocationViolations(lines) {
  const violations = [];
  const direct =
    /\b(?:execa|execaSync|execFile|execFileSync|spawn|spawnSync)\s*\(\s*["'`]([^"'`]+)["'`]/gu;
  const shell = /\b(?:exec|execSync)\s*\(\s*["'`]([^"'`]+)["'`]/gu;

  lines.forEach((line, index) => {
    for (const match of line.matchAll(direct)) {
      const tool = basename(match[1])
        .replace(/\.cmd$/iu, "")
        .toLowerCase();
      if (actions.has(tool)) violations.push({ line: index + 1, tool });
    }
    for (const match of line.matchAll(shell)) {
      const tool = commandViolation(match[1]);
      if (tool) violations.push({ line: index + 1, tool });
    }
  });

  return violations;
}

function scanFile(file) {
  const content = readFileSync(file, "utf8");
  if (content.includes("\0")) return [];
  const lines = content.split(/\r?\n/u);
  const extension = extname(file).toLowerCase();
  const name = basename(file).toLowerCase();

  if (extension === ".md" || extension === ".mdx") return markdownViolations(lines);
  if (name === "package.json" || extension === ".json") return packageScriptViolations(content);
  if ([".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx"].includes(extension)) {
    return sourceInvocationViolations(lines);
  }

  const shellLike =
    [".sh", ".bash", ".zsh"].includes(extension) ||
    ["dockerfile", "justfile", "makefile"].includes(name) ||
    lines[0]?.startsWith("#!");
  const yamlLike = extension === ".yaml" || extension === ".yml";
  if (!shellLike && !yamlLike) return [];

  return lines.flatMap((line, index) => {
    const tool = commandViolation(line);
    return tool ? [{ line: index + 1, tool }] : [];
  });
}

const files =
  explicitFiles.length > 0
    ? explicitFiles
    : repositoryFiles().filter((file) => existsSync(file) && !shouldIgnore(file));
const findings = files.flatMap((file) => scanFile(file).map((finding) => ({ file, ...finding })));

if (findings.length > 0) {
  process.stderr.write("Forbidden package-manager command usage found:\n");
  findings.forEach(({ file, line, tool }) => {
    process.stderr.write(`  ${relative(repositoryRoot, file)}:${line} [${tool}]\n`);
  });
  process.exit(1);
}

process.stdout.write(
  `No forbidden package-manager command usage found in ${files.length} files.\n`,
);
