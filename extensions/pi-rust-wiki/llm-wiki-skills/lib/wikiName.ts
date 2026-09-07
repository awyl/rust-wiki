import { execFileSync } from "node:child_process";

/**
 * Derive a deterministic wiki space name from the project's git history:
 * `<sanitized-first-commit-subject>-<short-hash>`, e.g. `rust-wiki-cc79119`.
 * Returns null when the name cannot be derived (not a repo, no commits,
 * empty subject).
 */
export function deriveWikiName(cwd: string): string | null {
  let hash: string;
  let subject: string;
  try {
    hash = execFileSync("git", ["-C", cwd, "rev-list", "--max-parents=0", "HEAD"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    })
      .trim()
      .split("\n")[0];
    subject = execFileSync("git", ["-C", cwd, "log", "-1", "--format=%s", hash], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    return null;
  }
  if (!hash || !subject) return null;
  const name = subject
    .replace(/^init(ial)?\b[:\s-]*/i, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  if (!name) return null;
  return `${name}-${hash.slice(0, 7)}`;
}
