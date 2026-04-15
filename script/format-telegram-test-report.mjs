import { existsSync, readFileSync } from "node:fs";

const logPath = process.env.LOG_PATH || "daily-testnet-ts.log";
const summaryPath =
  process.env.SUMMARY_PATH || "daily-testnet-ts-ai-summary.md";
const status = process.env.STATUS || "unknown";
const branch = process.env.BRANCH || "unknown";
const runUrl = process.env.RUN_URL || "unknown";
const title = process.env.REPORT_TITLE || "Daily Testnet TS";
const mode = process.env.REPORT_MODE || "testnet";
const ethLabel = process.env.REPORT_ETH_LABEL || "hoodi-reth";

function readIfExists(path) {
  return existsSync(path) ? readFileSync(path, "utf8") : "";
}

function extractTests(log) {
  const passed = log.match(/Tests\s+(\d+)\s+passed\s+\((\d+)\)/);
  if (passed) {
    return `${passed[1]}/${passed[2]}`;
  }

  const failed = log.match(/Tests\s+(\d+)\s+failed\s+\|\s+(\d+)\s+passed\s+\((\d+)\)/);
  if (failed) {
    return `${failed[2]}/${failed[3]} passed, ${failed[1]} failed`;
  }

  return "unknown";
}

function extractDuration(log) {
  const match = log.match(/Duration\s+([0-9.]+)(ms|s|m)/);
  if (!match) {
    return "unknown";
  }

  const value = Number(match[1]);
  const unit = match[2];
  if (!Number.isFinite(value)) {
    return `${match[1]}${unit}`;
  }

  const seconds =
    unit === "ms" ? value / 1_000 : unit === "m" ? value * 60 : value;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.round(seconds % 60);

  if (minutes === 0) {
    return `${remainingSeconds}s`;
  }

  return `${minutes}m ${remainingSeconds.toString().padStart(2, "0")}s`;
}

function extractCosts(log) {
  const match = log.match(/TEST_COSTS delta eth=([^\s]+) wvara=([^\s]+)/);
  if (!match) {
    return {
      eth: "unknown",
      wvara: "unknown",
    };
  }

  return {
    eth: match[1],
    wvara: match[2],
  };
}

const log = readIfExists(logPath);
const summary = readIfExists(summaryPath).trim() || "AI summary unavailable.";
const tests = extractTests(log);
const duration = extractDuration(log);
const costs = extractCosts(log);

const message = [
  title,
  `Status: ${status}`,
  `Tests: ${tests}`,
  `Duration: ${duration}`,
  `Cost ETH: ${costs.eth}`,
  `Cost WVARA: ${costs.wvara}`,
  `Mode: ${mode}`,
  `ETH: ${ethLabel}`,
  `Branch: ${branch}`,
  `Run: ${runUrl}`,
  "",
  summary,
].join("\n");

console.log(message);
