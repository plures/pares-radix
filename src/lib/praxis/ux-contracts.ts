/**
 * UX Journey Contracts
 *
 * Praxis expectations for user experience flows.
 * These are validated at runtime to prevent dead ends, enforce
 * prerequisites, and ensure plugins provide proper empty states.
 */

import type { Expectation, DataRequirement } from '../types/plugin.js';
import { getAllRoutes, getAllNavItems } from './plugin-loader.js';

// ─── Built-in UX Expectations ───────────────────────────────────────────────

export const builtinUxExpectations: Expectation[] = [
  {
    id: 'ux-no-dead-ends',
    domain: 'ux',
    description: 'Every page must be reachable from the sidebar or a parent page',
    severity: 'error',
    validate: () => {
      const routes = getAllRoutes();
      const navHrefs = new Set(getAllNavItems().map(n => n.href));
      // Every route must be in nav OR be a child of a route that is
      for (const route of routes) {
        const inNav = navHrefs.has(route.path);
        const parentInNav = [...navHrefs].some(href => route.path.startsWith(href + '/'));
        if (!inNav && !parentInNav) {
          console.warn(`[radix:ux] Route "${route.path}" (${route.pluginId}) is not reachable from navigation`);
          return false;
        }
      }
      return true;
    },
  },
  {
    id: 'ux-data-prereqs-have-empty-states',
    domain: 'ux',
    description: 'Pages with data requirements must specify empty states with fulfillment actions',
    severity: 'error',
    validate: () => {
      const routes = getAllRoutes();
      for (const route of routes) {
        if (!route.requires?.length) continue;
        for (const req of route.requires) {
          if (!req.emptyMessage || !req.fulfillHref || !req.fulfillLabel) {
            console.warn(
              `[radix:ux] Route "${route.path}" has data requirement "${req.type}" without proper empty state`,
            );
            return false;
          }
        }
      }
      return true;
    },
  },
  {
    id: 'ux-nav-items-resolve',
    domain: 'ux',
    description: 'All navigation items must point to registered routes',
    severity: 'warning',
    validate: () => {
      const routePaths = new Set(getAllRoutes().map(r => r.path));
      // Add base routes that radix provides
      routePaths.add('/');
      routePaths.add('/settings');
      routePaths.add('/help');

      for (const nav of getAllNavItems()) {
        if (!routePaths.has(nav.href) && !nav.href.startsWith('http')) {
          console.warn(`[radix:ux] Nav item "${nav.label}" points to unregistered route "${nav.href}"`);
          return false;
        }
      }
      return true;
    },
  },
];

// ─── Runtime Checks ─────────────────────────────────────────────────────────

/**
 * Check data requirements for a route and return unmet ones.
 */
export async function checkDataRequirements(
  requirements: DataRequirement[],
  dataCheck: (type: string) => Promise<number>,
): Promise<DataRequirement[]> {
  const unmet: DataRequirement[] = [];

  for (const req of requirements) {
    const count = await dataCheck(req.type);
    if (count < (req.minCount ?? 1)) {
      unmet.push(req);
    }
  }

  return unmet;
}

/**
 * Validate all UX expectations. Returns violations.
 */
export async function validateUxExpectations(
  pluginExpectations: Expectation[] = [],
): Promise<{ id: string; description: string; severity: string }[]> {
  const all = [...builtinUxExpectations, ...pluginExpectations.filter(e => e.domain === 'ux')];
  const violations: { id: string; description: string; severity: string }[] = [];

  for (const exp of all) {
    try {
      const ok = await exp.validate(null);
      if (!ok) {
        violations.push({
          id: exp.id,
          description: exp.description,
          severity: exp.severity,
        });
      }
    } catch (err) {
      violations.push({
        id: exp.id,
        description: `${exp.description} (threw: ${err})`,
        severity: 'error',
      });
    }
  }

  return violations;
}
