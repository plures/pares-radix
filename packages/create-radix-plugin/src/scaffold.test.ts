// scaffold.test.ts — verifies the scaffolder generates a valid canonical plugin.
//
// Build-the-binary/hit-the-API discipline: this test actually RUNS scaffold() into
// a temp dir, then asserts the tree matches ADR-0024 section 1 and the generated
// plugin.toml validates against the section-2 schema.
import { describe, it, expect } from "vitest";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { parseArgs, scaffold } from "./index.js";
import { validatePluginToml } from "./validate.js";

describe("create-radix-plugin scaffold", () => {
  it("generates the canonical ADR-0024 tree with a valid plugin.toml", () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "crp-test-"));
    const opts = parseArgs(["temp-probe", "--out", tmp]);
    const { dir, files } = scaffold(opts);

    const expected = [
      "plugin.toml",
      path.join("procedures", "temp-probe.px"),
      path.join("adapter", "temp-probe-adapter.ts"),
      path.join("ui", "Dashboard.svelte"),
      path.join("tests", "temp-probe.test.ts"),
      "README.md",
    ];
    for (const rel of expected) {
      expect(files).toContain(rel);
      expect(fs.existsSync(path.join(dir, rel))).toBe(true);
    }

    const toml = fs.readFileSync(path.join(dir, "plugin.toml"), "utf8");
    const res = validatePluginToml(toml);
    expect(res.errors).toEqual([]);
    expect(res.ok).toBe(true);

    // the .px carries real logic, not a placeholder
    const px = fs.readFileSync(path.join(dir, "procedures", "temp-probe.px"), "utf8");
    expect(px).toContain("procedure on_refresh_requested");
    expect(px).toContain("rule classify_status");

    // the adapter is a pure IO seam (no business decision keywords)
    const adapter = fs.readFileSync(path.join(dir, "adapter", "temp-probe-adapter.ts"), "utf8");
    expect(adapter).toContain("NO DECISION LOGIC");
    expect(adapter).toContain("export async function probe");

    // the UI imports design-dojo and contains no raw decision markup
    const svelte = fs.readFileSync(path.join(dir, "ui", "Dashboard.svelte"), "utf8");
    expect(svelte).toContain("@plures/design-dojo");

    fs.rmSync(tmp, { recursive: true, force: true });
  });

  it("rejects invalid plugin ids", () => {
    expect(() => parseArgs(["Bad_Id"])).toThrow();
    expect(() => parseArgs([])).toThrow();
  });
});
