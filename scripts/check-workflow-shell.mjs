#!/usr/bin/env node

import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { promisify } from "node:util";

import { parse } from "yaml";

const execFileAsync = promisify(execFile);
const githubExpression = /\$\{\{[\s\S]*?\}\}/gu;
const posixShell = /^(?:bash|sh)(?:\s|$)/u;

function record(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value : null;
}

function defaultPosixRunner(job) {
  const runner = job["runs-on"];
  return typeof runner === "string" && !runner.includes("${{") && !runner.startsWith("windows-");
}

function posixStep(job, step) {
  const shell = step.shell;
  return typeof shell === "string" ? posixShell.test(shell) : defaultPosixRunner(job);
}

function workflowScripts(workflow) {
  const jobs = record(workflow.jobs);
  if (jobs === null) return [];
  const scripts = [];
  for (const [jobName, jobValue] of Object.entries(jobs)) {
    const job = record(jobValue);
    if (job === null || !Array.isArray(job.steps)) continue;
    for (const [index, stepValue] of job.steps.entries()) {
      const step = record(stepValue);
      if (step !== null && typeof step.run === "string" && posixStep(job, step)) {
        scripts.push({ jobName, run: step.run, stepIndex: index + 1 });
      }
    }
  }
  return scripts;
}

function safeName(value) {
  return value.replaceAll(/[^A-Za-z0-9_.-]/gu, "-");
}

function shellSource(source, script) {
  const label = `${source}:${script.jobName}:step-${String(script.stepIndex)}`;
  const body = script.run.replaceAll(githubExpression, "BLOBYARD_GITHUB_EXPRESSION");
  return `#!/usr/bin/env bash\n# ${label}\n${body}\n`;
}

async function extractedScripts(directory, workflowPath) {
  const content = await readFile(workflowPath, "utf8");
  const workflow = record(parse(content));
  if (workflow === null) throw new Error(`${workflowPath} must contain a workflow object.`);
  const files = [];
  for (const script of workflowScripts(workflow)) {
    const name = `${safeName(basename(workflowPath))}-${safeName(script.jobName)}-${String(script.stepIndex)}.sh`;
    const path = join(directory, name);
    await writeFile(path, shellSource(workflowPath, script), { mode: 0o600 });
    files.push(path);
  }
  return files;
}

async function main() {
  const workflowPaths = process.argv.slice(2);
  if (workflowPaths.length === 0) throw new Error("At least one workflow path is required.");
  const directory = await mkdtemp(join(tmpdir(), "blobyard-workflow-shell-"));
  try {
    const nested = await Promise.all(
      workflowPaths.map((workflowPath) => extractedScripts(directory, workflowPath)),
    );
    const files = nested.flat();
    if (files.length > 0) await execFileAsync("shellcheck", ["-x", ...files]);
  } catch (error) {
    if (typeof error?.stdout === "string") process.stdout.write(error.stdout);
    if (typeof error?.stderr === "string") process.stderr.write(error.stderr);
    throw error;
  } finally {
    await rm(directory, { force: true, recursive: true });
  }
}

main().catch((error) => {
  const message = error instanceof Error ? error.message : "Workflow shell validation failed.";
  process.stderr.write(`${message}\n`);
  process.exitCode = 1;
});
