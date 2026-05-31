import { test, expect } from '@playwright/test';

test.describe('smoke tests', () => {
	test('page loads successfully', async ({ page }) => {
		const errors: string[] = [];
		page.on('console', (msg) => {
			if (msg.type() === 'error') errors.push(msg.text());
		});

		await page.goto('/');
		await expect(page).toHaveTitle(/radix|pares|praxis/i);
		expect(errors).toHaveLength(0);
	});

	test('no unhandled JS errors on load', async ({ page }) => {
		const errors: Error[] = [];
		page.on('pageerror', (err) => errors.push(err));

		await page.goto('/');
		await page.waitForLoadState('networkidle');
		expect(errors).toHaveLength(0);
	});
});
