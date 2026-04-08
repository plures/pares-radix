import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const isTauri = process.env.TAURI_ENV_PLATFORM !== undefined;

export default defineConfig({
	plugins: [sveltekit()],
	// Tauri requires a fixed port for its devUrl and cannot use HMR websockets
	// on a different port. Disable HMR overlay when building inside Tauri.
	...(isTauri
		? {
				clearScreen: false,
				server: {
					port: 5173,
					strictPort: true,
					watch: {
						// Tell Vite to ignore watching `src-tauri`
						ignored: ['**/src-tauri/**'],
					},
				},
			}
		: {}),
});
