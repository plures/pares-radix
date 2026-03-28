// Theme store — dark/light mode, persisted to localStorage
import { browser } from '$app/environment';

function createThemeStore() {
	let current = $state<'light' | 'dark'>(
		browser ? (localStorage.getItem('radix-theme') as 'light' | 'dark') ?? 'dark' : 'dark'
	);

	$effect(() => {
		if (browser) {
			localStorage.setItem('radix-theme', current);
			document.documentElement.setAttribute('data-theme', current);
		}
	});

	return {
		get value() { return current; },
		set value(v: 'light' | 'dark') { current = v; },
		toggle() { current = current === 'dark' ? 'light' : 'dark'; }
	};
}

export const theme = createThemeStore();
