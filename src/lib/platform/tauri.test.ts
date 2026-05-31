import { describe, it, expect } from 'vitest';
import { isTauri } from './tauri.js';

describe('tauri platform bridge', () => {
	describe('isTauri()', () => {
		it('returns false in jsdom (no __TAURI_INTERNALS__)', () => {
			expect(isTauri()).toBe(false);
		});
	});

	describe('tauri functions are no-ops in browser', async () => {
		// Dynamic import to get the full module
		const tauri = await import('./tauri.js');

		it('navigate() resolves without error', async () => {
			if (tauri.navigate) {
				await expect(tauri.navigate('/test')).resolves.not.toThrow();
			}
		});

		it('saveWindowState() resolves without error', async () => {
			if (tauri.saveWindowState) {
				await expect(
					tauri.saveWindowState({ x: 0, y: 0, width: 800, height: 600, maximized: false })
				).resolves.not.toThrow();
			}
		});

		it('getWindowState() resolves to null or undefined', async () => {
			if (tauri.getWindowState) {
				const state = await tauri.getWindowState();
				expect(state).toBeNull();
			}
		});
	});
});
