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

		it('tauriNavigate() resolves without error', async () => {
			await expect(tauri.tauriNavigate('/test')).resolves.not.toThrow();
		});

		it('tauriSaveWindowState() resolves without error', async () => {
			await expect(
				tauri.tauriSaveWindowState({ x: 0, y: 0, width: 800, height: 600, maximized: false })
			).resolves.not.toThrow();
		});

		it('tauriGetWindowState() resolves to null', async () => {
			const state = await tauri.tauriGetWindowState();
			expect(state).toBeNull();
		});
	});
});
