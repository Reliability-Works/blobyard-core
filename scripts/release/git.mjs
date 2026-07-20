import { execFileSync } from "node:child_process";

export function git(repo, args) {
  return execFileSync("git", args, { cwd: repo, encoding: "utf8" }).trim();
}

export function lines(value) {
  return value.split("\n").filter(Boolean);
}
