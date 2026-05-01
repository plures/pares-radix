/**
 * Breadcrumb store — allows plugins and pages to set contextual breadcrumbs.
 */

interface Crumb {
	label: string;
	href?: string;
}

let current: Crumb[] = $state([]);

export const breadcrumbs = {
	get value() {
		return current;
	},
	set(crumbs: Crumb[]) {
		current = crumbs;
	},
	reset() {
		current = [];
	},
};
