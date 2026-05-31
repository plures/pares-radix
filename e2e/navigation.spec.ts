import { test, expect } from '@playwright/test';

const routes = ['/', '/canvas', '/design', '/inventory', '/plugins', '/settings', '/help'];

test.describe('navigation', () => {
	test('sidebar renders navigation items', async ({ page }) => {
		await page.goto('/');
		const nav = page.locator('nav, [role="navigation"], aside');
		await expect(nav.first()).toBeVisible();
	});

	for (const route of routes) {
		test(`navigates to ${route}`, async ({ page }) => {
			await page.goto(route);
			await page.waitForLoadState('networkidle');
			// Page should not show a 404 or error state
			const body = await page.textContent('body');
			expect(body).not.toContain('404');
		});
	}
});
