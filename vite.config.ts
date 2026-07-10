import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const isTauri = process.env.TAURI_ENV_PLATFORM !== undefined;

export default defineConfig({
	plugins: [sveltekit()],
	// Serve the linked workspace UI package from source instead of pre-bundling
	// it. Pre-bundling caches a stale dep chunk that omits newly-added exports
	// (e.g. WorkspaceLayout) after design-dojo source changes, throwing a
	// "does not provide an export named ..." SyntaxError until the vite cache is
	// manually cleared. Excluding it keeps the package HMR-fresh.
	optimizeDeps: {
		exclude: ['@plures/design-dojo'],
	},
	// Tauri requires a fixed port for its devUrl and cannot use HMR websockets
	// on a different port. Disable HMR overlay when building inside Tauri.
	...(isTauri
		? {
				clearScreen: false,
				server: {
					port: 5173,
					strictPort: true,
					watch: {
						// Tell Vite to ignore watching `src-tauri` and the Rust build
						// output. Watching `target/` on Windows crashes the dev watcher
						// with EBUSY when cargo is writing build-script .exe files.
						ignored: ['**/src-tauri/**', '**/target/**'],
					},
				},
			}
		: {}),
});
