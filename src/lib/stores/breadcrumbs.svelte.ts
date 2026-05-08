/**
 * Breadcrumb store — allows plugins and pages to set contextual breadcrumbs.
 */

import { getSharedGraph } from './plures-db-adapter.js';

type Subscriber<T> = (value: T) => void;

export interface Crumb {
	label: string;
	href?: string;
}

const BREADCRUMB_KEY = 'radix-breadcrumbs';

const graph = getSharedGraph();
const stored = graph.get(BREADCRUMB_KEY);
let current: Crumb[] = Array.isArray(stored) ? stored : [];
const subscribers = new Set<Subscriber<Crumb[]>>();

function notify() {
	for (const sub of subscribers) sub(current);
}

function persist() {
	graph.put(BREADCRUMB_KEY, current);
}

const crumbsStore = {
	subscribe(run: Subscriber<Crumb[]>) {
		run(current);
		subscribers.add(run);
		return () => subscribers.delete(run);
	},
};

export const breadcrumbs = {
	crumbs: crumbsStore,
	get value() {
		return current;
	},
	set(crumbs: Crumb[]) {
		current = crumbs;
		persist();
		notify();
	},
	reset() {
		current = [];
		persist();
		notify();
	},
};
