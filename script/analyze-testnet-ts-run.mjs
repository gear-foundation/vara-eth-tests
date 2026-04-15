import { readFileSync, writeFileSync, existsSync } from "node:fs";

const OPENAI_API_KEY = process.env.OPENAI_API_KEY;
const OPENAI_MODEL = process.env.OPENAI_MODEL || "gpt-5.2";
const LOG_PATH = process.env.LOG_PATH || "daily-testnet-ts.log";
const OUTPUT_PATH =
  process.env.OUTPUT_PATH || "daily-testnet-ts-ai-summary.md";

if (!OPENAI_API_KEY) {
  throw new Error("OPENAI_API_KEY is required");
}

function readText(path, maxChars) {
  if (!existsSync(path)) {
    return `File not found: ${path}`;
  }

  const content = readFileSync(path, "utf8");
  if (content.length <= maxChars) {
    return content;
  }

  return content.slice(content.length - maxChars);
}

function extractResponseText(data) {
  if (typeof data.output_text === "string" && data.output_text.trim()) {
    return data.output_text.trim();
  }

  const parts = [];
  for (const item of data.output ?? []) {
    for (const content of item.content ?? []) {
      if (
        (content.type === "output_text" || content.type === "text") &&
        typeof content.text === "string"
      ) {
        parts.push(content.text);
      }
    }
  }

  return parts.join("\n").trim();
}

const agents = readText("AGENTS.md", 12_000);
const workflow = readText(".github/workflows/daily-testnet-ts.yml", 8_000);
const runner = readText("script/run-testnet-daily-ts.sh", 6_000);
const balanceTest = readText("test/balance.test.ts", 6_000);
const vftTest = readText("test/vft/vft.test.ts", 12_000);
const log = readText(LOG_PATH, 24_000);

const prompt = `
You are analyzing a GitHub Actions run for this repository.

Return a concise plain-text report in exactly this format:
Summary: <one short paragraph>
Risk: <one short paragraph>
Next: <one short paragraph>

Rules:
- Be concrete and repository-specific.
- If the run succeeded, say that clearly and mention any residual risk.
- If the run failed, identify the most likely failing layer: config, CI bootstrap, RPC/network, test logic, or flaky external dependency.
- Prefer evidence from the log over speculation.
- Optimize for Telegram readability.
- Keep the whole report under 90 words.
- Each line should be compact and easy to scan on mobile.
- Do not use Markdown headings, bullets, numbering, or code fences.

Repository rules:
${agents}

Workflow:
${workflow}

Runner script:
${runner}

Relevant test files:
=== test/balance.test.ts ===
${balanceTest}

=== test/vft/vft.test.ts ===
${vftTest}

Run log:
${log}
`;

const response = await fetch("https://api.openai.com/v1/responses", {
  method: "POST",
  headers: {
    "Content-Type": "application/json",
    Authorization: `Bearer ${OPENAI_API_KEY}`,
  },
  body: JSON.stringify({
    model: OPENAI_MODEL,
    input: prompt,
  }),
});

if (!response.ok) {
  const body = await response.text();
  throw new Error(`OpenAI API error ${response.status}: ${body}`);
}

const data = await response.json();
const summary = extractResponseText(data);

if (!summary) {
  throw new Error("OpenAI API returned an empty analysis");
}

writeFileSync(OUTPUT_PATH, `${summary}\n`);
console.log(summary);
