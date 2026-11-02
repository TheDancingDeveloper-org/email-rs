import fs from "node:fs";

const [reportPath, waiverPath] = process.argv.slice(2);
if (!reportPath || !waiverPath) {
  console.error("usage: node scripts/check-npm-audit.mjs <report.json> <waivers.json>");
  process.exit(2);
}

const report = JSON.parse(fs.readFileSync(reportPath, "utf8"));
const waiverFile = JSON.parse(fs.readFileSync(waiverPath, "utf8"));
const today = new Date().toISOString().slice(0, 10);
const waivers = new Map(waiverFile.waivers.map((waiver) => [waiver.id, waiver]));
const findings = new Map();

for (const vulnerability of Object.values(report.vulnerabilities ?? {})) {
  for (const cause of vulnerability.via ?? []) {
    if (typeof cause !== "object" || !["high", "critical"].includes(cause.severity)) continue;
    const id = cause.url?.split("/").pop();
    if (id) findings.set(id, cause);
  }
}

const unwaived = [...findings.keys()].filter((id) => {
  const waiver = waivers.get(id);
  return !waiver || waiver.expires < today || !waiver.reason;
});

if (unwaived.length > 0) {
  console.error(`Unwaived high/critical npm advisories: ${unwaived.sort().join(", ")}`);
  process.exit(1);
}

console.log(`Accepted ${findings.size} documented high/critical npm advisory waiver(s).`);
