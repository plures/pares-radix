/**
 * Data Transfer — export/import format definitions and validation.
 *
 * Defines the canonical RadixExport JSON envelope consumed and produced
 * by the settings page.  Import validation lives here so it can be
 * unit-tested independently of the UI.
 */

// Bump this constant whenever the envelope schema changes in a breaking way
// (e.g. a required field is renamed, removed, or its type changes).
// Increment to '2', '3', … and update validateImport to reject older versions
// or to run a migration path.  Backwards-compatible additions (new optional
// fields) do NOT require a version bump.
export const EXPORT_FORMAT_VERSION = '1' as const;

// ─── Types ───────────────────────────────────────────────────────────────────

export interface PluginManifestEntry {
  id: string;
  name: string;
  version: string;
  icon: string;
}

export interface RadixExportMeta {
  /** Format version — used by validateImport to detect incompatible files. */
  version: typeof EXPORT_FORMAT_VERSION;
  /** ISO 8601 timestamp of when the export was created. */
  exportedAt: string;
  /** Snapshot of all active plugins at export time. */
  plugins: PluginManifestEntry[];
}

export interface RadixExport {
  /** Radix envelope — always present, used for format detection. */
  _radix: RadixExportMeta;
  /** All persisted settings keys. */
  settings: Record<string, unknown>;
  /** Per-plugin data slices, keyed by plugin ID. */
  plugins: Record<string, unknown>;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/**
 * Assemble a complete RadixExport envelope from its parts.
 */
export function createExport(
  settings: Record<string, unknown>,
  pluginData: Record<string, unknown>,
  activePlugins: PluginManifestEntry[],
): RadixExport {
  return {
    _radix: {
      version: EXPORT_FORMAT_VERSION,
      exportedAt: new Date().toISOString(),
      plugins: activePlugins,
    },
    settings,
    plugins: pluginData,
  };
}

/**
 * Return true when `data` is a structurally valid RadixExport.
 *
 * Checks for the `_radix` envelope and a matching format version.
 * Does NOT deep-validate per-plugin payloads — each plugin owns that
 * responsibility inside its own `onDataImport` hook.
 */
export function validateImport(data: unknown): data is RadixExport {
  if (!data || typeof data !== 'object') return false;
  const d = data as Record<string, unknown>;
  if (!d._radix || typeof d._radix !== 'object') return false;
  const meta = d._radix as Record<string, unknown>;
  return meta.version === EXPORT_FORMAT_VERSION;
}
