// Only synthetic fixtures and their mutations enter this CI-only workflow.
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const targets = ["split_parse", "load_str", "csl_export"];
const limits = ["-timeout=5", "-rss_limit_mb=1024", "-max_len=16384"];

export function failure(target, log) {
  const summary = log.match(/(?:ERROR|SUMMARY): (?:libFuzzer|AddressSanitizer|UndefinedBehaviorSanitizer): ([^\n]+)/)?.[1];
  if (!summary) return null; // A build/tool failure must fail CI, never become a parser issue.
  const kind = /timeout/.test(summary) ? "timeout" : /out-of-memory/.test(summary) ? "out-of-memory" : "crash";
  // Symbol names survive ASLR, checkout paths and line-number changes. Ignore
  // runtime frames; resource failures have no reliable backtrace.
  const frames = kind === "crash" ? [...log.matchAll(/\bin ((?:ibis_core|biblatex|split_parse::lexer)::[^\s(]+)/g)].slice(0, 4).map((m) => m[1].replace(/::h[0-9a-f]+$/, "")) : [];
  const detail = kind === "crash" ? summary.replace(/0x[0-9a-f]+/gi, "ADDRESS").replace(/\d+/g, "N") : kind;
  const signature = createHash("sha256").update([target, detail, ...frames].join("\n")).digest("hex");
  return { kind, signature, detail: [detail, ...frames].join("\n") };
}

// Keep the original signature during deletion reduction, including resource
// failures that have no sanitizer dedup token. Bound the entire reduction.
export function minimize(input, reproduce, signature, budgetMs = 120_000) {
  const deadline = Date.now() + budgetMs;
  for (let width = Math.ceil(input.length / 2); width >= 1 && Date.now() < deadline; width = Math.floor(width / 2)) {
    for (let start = 0; start < input.length && Date.now() < deadline;) {
      const candidate = Buffer.concat([input.subarray(0, start), input.subarray(start + width)]);
      if (reproduce(candidate) === signature) input = candidate;
      else start += width;
    }
  }
  return input;
}

export async function report(repository, target, input, problem, runUrl, request) {
  const marker = `<!-- fuzz:${problem.signature} -->`;
  for (let page = 1; ; page++) {
    const issues = await request(`/repos/${repository}/issues?state=open&labels=area%3Atesting&per_page=100&page=${page}`);
    if (issues.some((issue) => issue.body?.includes(marker))) return false;
    if (issues.length < 100) break;
  }
  const encoded = input.toString("base64");
  await request(`/repos/${repository}/issues`, {
    title: `fuzz: ${target} ${problem.kind} (${problem.signature.slice(0, 12)})`,
    labels: ["area:testing"],
    body: `${marker}\nNightly fuzzing found a ${problem.kind} in \`${target}\`. [Workflow run](${runUrl}).\n\nFailure signature:\n\n    ${problem.detail.replaceAll("\n", "\n    ")}\n\nMinimized input (${input.length} bytes, lossless base64):\n\n    ${encoded}\n\nDecode the base64 into \`reproducer\`, then run:\n\n    cargo +nightly fuzz run ${target} reproducer -- ${limits.join(" ")}\n\nMinimization deletes bytes while preserving the failure signature, bounded to two minutes; reduction may stop at that budget. Before closing a fix, add the input to \`fixtures/bibtex/\` with a regression test.`,
  });
  return true;
}

async function run(target) {
  if (!targets.includes(target)) throw new Error("Unknown fuzz target");
  const corpus = `fuzz/corpus/${target}`;
  const artifacts = `fuzz/artifacts/${target}`;
  const results = `fuzz/results/${target}`;
  for (const dir of [corpus, artifacts, results]) mkdirSync(dir, { recursive: true });
  for (const file of readdirSync("fixtures/bibtex").filter((file) => file.endsWith(".bib"))) {
    copyFileSync(`fixtures/bibtex/${file}`, `${corpus}/seed-${file}`);
  }
  const command = (args, timeout) => spawnSync("cargo", ["+nightly", "fuzz", ...args], {
    encoding: "utf8", timeout, killSignal: "SIGKILL", maxBuffer: 16 * 1024 * 1024,
  });
  const build = command(["build", target], 300_000);
  if (build.status !== 0) throw new Error(`Fuzz build failed: ${build.stderr}`);
  const result = command(["run", target, corpus, "--", ...limits, "-max_total_time=300"], 330_000);
  const log = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
  writeFileSync(`${results}/run.log`, log);
  if (result.status === 0) return;
  const problem = failure(target, log);
  const files = readdirSync(artifacts).filter((file) => /^(crash|timeout|oom|leak)-/.test(file));
  if (!problem || files.length === 0) throw new Error("Fuzzing failed without a classified reproducer; see run.log");
  for (const file of files) {
    const candidatePath = `${results}/candidate`;
    const reproduce = (data) => {
      writeFileSync(candidatePath, data);
      const attempt = command(["run", target, candidatePath, "--", ...limits, "-runs=1"], 15_000);
      return attempt.status === 0 ? null : failure(target, `${attempt.stdout}\n${attempt.stderr}`)?.signature;
    };
    const original = readFileSync(`${artifacts}/${file}`);
    if (reproduce(original) !== problem.signature) throw new Error("Failure did not reproduce; see preserved artifact");
    const minimized = minimize(original, reproduce, problem.signature);
    writeFileSync(`${results}/${file}.min`, minimized);
    if (process.env.GITHUB_TOKEN) {
      await report(process.env.GITHUB_REPOSITORY, target, minimized, problem,
        `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}`, async (path, body) => {
          const response = await fetch(`https://api.github.com${path}`, {
            method: body ? "POST" : "GET", headers: { Authorization: `Bearer ${process.env.GITHUB_TOKEN}`, "Content-Type": "application/json", "X-GitHub-Api-Version": "2022-11-28" },
            body: body ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(30_000),
          });
          if (!response.ok) throw new Error(`Issue reporting failed: HTTP ${response.status}`);
          return response.json();
        });
    }
  }
  throw new Error("Fuzzing found a failure; minimized reproducer saved and issue reported when credentials are available");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await run(process.argv[2]);
}
