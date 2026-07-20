// validate.ts — assert a generated plugin.toml conforms to ADR-0024 section 2.
//
// This is a real structural validator (not a stub): it parses the TOML and checks
// the required tables/keys the ADR-0024 section-2 schema mandates. Used by the
// scaffolder's own verification test and runnable standalone against any plugin.
//
//   tsx src/validate.ts <path-to-plugin.toml>

import * as fs from "node:fs";
import { parse } from "smol-toml";

export interface ValidationResult {
  ok: boolean;
  errors: string[];
}

export function validatePluginToml(text: string): ValidationResult {
  const errors: string[] = [];
  let doc: Record<string, unknown>;
  try {
    doc = parse(text) as Record<string, unknown>;
  } catch (e) {
    return { ok: false, errors: [`TOML parse error: ${(e as Error).message}`] };
  }

  const asObj = (v: unknown): Record<string, unknown> | undefined =>
    v && typeof v === "object" && !Array.isArray(v)
      ? (v as Record<string, unknown>)
      : undefined;

  // [plugin] table + required fields (ADR-0024 section 2)
  const plugin = asObj(doc.plugin);
  if (!plugin) {
    errors.push("missing [plugin] table");
  } else {
    for (const k of ["id", "name", "version", "description"]) {
      if (typeof plugin[k] !== "string" || !(plugin[k] as string).length) {
        errors.push(`[plugin].${k} must be a non-empty string`);
      }
    }
    if (plugin.trust !== undefined) {
      const t = plugin.trust;
      if (t !== "verified" && t !== "community" && t !== "local") {
        errors.push(`[plugin].trust must be verified|community|local (got ${String(t)})`);
      }
    }
  }

  // [capabilities.*] present (required/optional/provided tables per section 2)
  const caps = asObj(doc.capabilities);
  if (!caps) {
    errors.push("missing [capabilities] tables (required/optional/provided)");
  } else {
    for (const sub of ["required", "optional", "provided"]) {
      if (asObj(caps[sub]) === undefined) {
        errors.push(`missing [capabilities.${sub}] table`);
      }
    }
  }

  // [permissions] table (ADR-0011 closed set)
  if (!asObj(doc.permissions)) {
    errors.push("missing [permissions] table");
  }

  // [dependencies] with plugins[] and capabilities[] (section 3)
  const deps = asObj(doc.dependencies);
  if (!deps) {
    errors.push("missing [dependencies] table");
  } else {
    if (!Array.isArray(deps.plugins)) errors.push("[dependencies].plugins must be an array");
    if (!Array.isArray(deps.capabilities))
      errors.push("[dependencies].capabilities must be an array");
  }

  // [[contributes.routes]] and [[contributes.navItems]] (UI contribution surface)
  const contributes = asObj(doc.contributes);
  if (!contributes) {
    errors.push("missing [contributes] (routes/navItems)");
  } else {
    const routes = contributes.routes;
    if (!Array.isArray(routes) || routes.length === 0) {
      errors.push("[[contributes.routes]] must have at least one entry");
    } else {
      for (const r of routes as Record<string, unknown>[]) {
        if (typeof r.path !== "string" || typeof r.component !== "string") {
          errors.push("each [[contributes.routes]] needs string path + component");
          break;
        }
      }
    }
    const nav = contributes.navItems;
    if (!Array.isArray(nav) || nav.length === 0) {
      errors.push("[[contributes.navItems]] must have at least one entry");
    }
  }

  return { ok: errors.length === 0, errors };
}

// CLI
import { fileURLToPath } from "node:url";
import * as path from "node:path";
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const p = process.argv[2];
  if (!p) {
    console.error("usage: tsx src/validate.ts <path-to-plugin.toml>");
    process.exit(2);
  }
  const res = validatePluginToml(fs.readFileSync(p, "utf8"));
  if (res.ok) {
    console.log(`VALID: ${p} conforms to ADR-0024 section 2`);
  } else {
    console.error(`INVALID: ${p}`);
    for (const e of res.errors) console.error(`  - ${e}`);
    process.exit(1);
  }
}
