import { describe, it, expect } from 'vitest';
import { applyDelta, diffField } from './schema-delta.js';
import type { EntitySchema } from './types-local.js';

const base: EntitySchema = {
	name: 'Item',
	fields: [
		{ name: 'title', type: 'string', required: true },
		{ name: 'qty', type: 'number' },
		{ name: 'active', type: 'boolean' },
	],
};

describe('applyDelta', () => {
	it('does not mutate the input schema', () => {
		const before = JSON.parse(JSON.stringify(base));
		applyDelta(base, { op: 'add_field', field: { name: 'x', type: 'string' } });
		expect(base).toEqual(before);
	});

	it('add_field appends a new field', () => {
		const next = applyDelta(base, { op: 'add_field', field: { name: 'notes', type: 'string' } });
		expect(next.fields.map((f) => f.name)).toEqual(['title', 'qty', 'active', 'notes']);
	});

	it('add_field rejects a duplicate name', () => {
		expect(() => applyDelta(base, { op: 'add_field', field: { name: 'qty', type: 'number' } })).toThrow(
			/already exists/,
		);
	});

	it('rename_field renames in place preserving order + attrs', () => {
		const next = applyDelta(base, { op: 'rename_field', from: 'title', to: 'name' });
		expect(next.fields[0]).toMatchObject({ name: 'name', type: 'string', required: true });
	});

	it('rename_field rejects collision with an existing field', () => {
		expect(() => applyDelta(base, { op: 'rename_field', from: 'title', to: 'qty' })).toThrow(
			/already exists/,
		);
	});

	it('retype_field changes type and drops stale select options', () => {
		const withSelect: EntitySchema = {
			fields: [{ name: 'color', type: 'select', options: [{ value: 'r', label: 'Red' }] }],
		};
		const next = applyDelta(withSelect, { op: 'retype_field', name: 'color', to: 'string' });
		expect(next.fields[0].type).toBe('string');
		expect(next.fields[0].options).toBeUndefined();
	});

	it('remove_field removes by name', () => {
		const next = applyDelta(base, { op: 'remove_field', name: 'qty' });
		expect(next.fields.map((f) => f.name)).toEqual(['title', 'active']);
	});

	it('remove_field rejects unknown field', () => {
		expect(() => applyDelta(base, { op: 'remove_field', name: 'nope' })).toThrow(/Unknown field/);
	});

	it('reorder_field moves a field to a new index', () => {
		const next = applyDelta(base, { op: 'reorder_field', name: 'active', toIndex: 0 });
		expect(next.fields.map((f) => f.name)).toEqual(['active', 'title', 'qty']);
	});

	it('reorder_field clamps out-of-range indexes', () => {
		const next = applyDelta(base, { op: 'reorder_field', name: 'title', toIndex: 99 });
		expect(next.fields.map((f) => f.name)).toEqual(['qty', 'active', 'title']);
	});

	it('update_field replaces the whole field definition', () => {
		const next = applyDelta(base, {
			op: 'update_field',
			name: 'qty',
			field: { name: 'quantity', type: 'number', required: true },
		});
		expect(next.fields[1]).toEqual({ name: 'quantity', type: 'number', required: true });
	});
});

describe('diffField', () => {
	it('emits add_field when there is no prior field', () => {
		const d = diffField(undefined, { name: 'a', type: 'string' });
		expect(d).toEqual({ op: 'add_field', field: { name: 'a', type: 'string' } });
	});

	it('emits update_field keyed by the old name when editing', () => {
		const d = diffField({ name: 'a', type: 'string' }, { name: 'b', type: 'number' });
		expect(d).toEqual({ op: 'update_field', name: 'a', field: { name: 'b', type: 'number' } });
	});
});
