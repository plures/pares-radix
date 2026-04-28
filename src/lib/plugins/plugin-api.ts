/**
 * Tauri IPC bindings for plugin management and entity CRUD.
 *
 * These functions call the Rust Tauri commands defined in
 * `crates/tauri-app/src/plugins.rs`.
 */
import { invoke } from '@tauri-apps/api/core';

// ── Types ────────────────────────────────────────────────────────────────────

export interface PluginInfo {
	name: string;
	version: string;
	description: string;
	entities: EntityInfo[];
}

export interface EntityInfo {
	name: string;
	display_name: string;
	fields: FieldInfo[];
	icon?: string;
}

export interface FieldInfo {
	name: string;
	field_type: string;
	required: boolean;
	description?: string;
}

// ── Plugin management ────────────────────────────────────────────────────────

export async function installPlugin(path: string): Promise<string> {
	return invoke('plugin_install', { path });
}

export async function listPlugins(): Promise<PluginInfo[]> {
	return invoke('plugin_list');
}

export async function uninstallPlugin(name: string): Promise<void> {
	return invoke('plugin_uninstall', { name });
}

export async function pluginSchema(name: string): Promise<string> {
	return invoke('plugin_schema', { name });
}

// ── Entity CRUD ──────────────────────────────────────────────────────────────

export async function createEntity(
	plugin: string,
	entityType: string,
	fields: Record<string, unknown>
): Promise<string> {
	return invoke('plugin_crud_create', { plugin, entityType, fields });
}

export async function listEntities(
	plugin: string,
	entityType: string
): Promise<Record<string, unknown>[]> {
	return invoke('plugin_crud_list', { plugin, entityType });
}

export async function updateEntity(
	entityId: string,
	fields: Record<string, unknown>
): Promise<void> {
	return invoke('plugin_crud_update', { entityId, fields });
}

export async function deleteEntity(entityId: string): Promise<void> {
	return invoke('plugin_crud_delete', { entityId });
}

export async function searchEntities(
	query: string,
	plugin: string
): Promise<Record<string, unknown>[]> {
	return invoke('plugin_crud_search', { query, plugin });
}
