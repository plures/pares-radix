/**
 * Schema-delta primitives (Phase B, design-dojo data kit).
 *
 * The `SchemaDesigner`/`FieldEditor` surfaces are *controlled components* over an
 * `EntitySchema` prop — they own no persistent state. Every user (or agens)
 * edit is expressed as a typed `SchemaDelta` operation that the HOST persists to
 * PluresDB (C-PLURES-003) and attaches a `.px` migration rule to for existing
 * rows (ADR-0031 §4/§6). Row migration is NOT implemented here — this module only
 * computes the next schema + a clean, replayable delta.
 *
 * The operation vocabulary mirrors the ADR-0031 §6 capability operations so a
 * user's manual customization and an agens-driven customization travel the exact
 * same path: `add_field`, `rename_field`, `retype_field`, `update_field`,
 * `remove_field`, `reorder_field`.
 */
import type { EntitySchema, SchemaField, SchemaFieldType } from './types-local.js';

export type SchemaDelta =
  | { op: 'add_field'; field: SchemaField }
  | { op: 'update_field'; name: string; field: SchemaField }
  | { op: 'rename_field'; from: string; to: string }
  | { op: 'retype_field'; name: string; to: SchemaFieldType }
  | { op: 'remove_field'; name: string }
  | { op: 'reorder_field'; name: string; toIndex: number };

/** Apply a single delta to a schema, returning a NEW schema (input untouched). */
export function applyDelta(schema: EntitySchema, delta: SchemaDelta): EntitySchema {
  const fields = schema.fields.map((f) => ({ ...f }));
  const indexOf = (name: string) => fields.findIndex((f) => f.name === name);

  switch (delta.op) {
    case 'add_field': {
      if (indexOf(delta.field.name) !== -1) {
        throw new Error(`Field "${delta.field.name}" already exists`);
      }
      fields.push({ ...delta.field });
      break;
    }
    case 'update_field': {
      const i = indexOf(delta.name);
      if (i === -1) throw new Error(`Unknown field "${delta.name}"`);
      if (delta.field.name !== delta.name && indexOf(delta.field.name) !== -1) {
        throw new Error(`Field "${delta.field.name}" already exists`);
      }
      fields[i] = { ...delta.field };
      break;
    }
    case 'rename_field': {
      const i = indexOf(delta.from);
      if (i === -1) throw new Error(`Unknown field "${delta.from}"`);
      if (delta.to !== delta.from && indexOf(delta.to) !== -1) {
        throw new Error(`Field "${delta.to}" already exists`);
      }
      fields[i] = { ...fields[i], name: delta.to };
      break;
    }
    case 'retype_field': {
      const i = indexOf(delta.name);
      if (i === -1) throw new Error(`Unknown field "${delta.name}"`);
      fields[i] = { ...fields[i], type: delta.to };
      if (delta.to !== 'select') delete fields[i].options;
      break;
    }
    case 'remove_field': {
      const i = indexOf(delta.name);
      if (i === -1) throw new Error(`Unknown field "${delta.name}"`);
      fields.splice(i, 1);
      break;
    }
    case 'reorder_field': {
      const i = indexOf(delta.name);
      if (i === -1) throw new Error(`Unknown field "${delta.name}"`);
      const [moved] = fields.splice(i, 1);
      const clamped = Math.max(0, Math.min(delta.toIndex, fields.length));
      fields.splice(clamped, 0, moved);
      break;
    }
  }

  return { ...schema, fields };
}

/**
 * Diff a `before`→`after` single-field edit into the minimal delta.
 * Used by `FieldEditor`/`SchemaDesigner` to emit a typed op rather than the whole
 * schema. `before === undefined` means the field is being added.
 */
export function diffField(before: SchemaField | undefined, after: SchemaField): SchemaDelta {
  if (!before) return { op: 'add_field', field: { ...after } };
  return { op: 'update_field', name: before.name, field: { ...after } };
}
