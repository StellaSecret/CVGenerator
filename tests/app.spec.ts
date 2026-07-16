import { test, expect } from '@playwright/test';
import { clearStorage } from './helpers';

test.describe('CV Generator App', () => {

  test.beforeEach(({ page }) => clearStorage(page));

  test('home page shows title', async ({ page }) => {
    await page.goto('/CVGenerator/');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('body')).toContainText(/CV|Generator|CV Generator/i);
  });

  test('sync page shows export/import buttons', async ({ page }) => {
    await page.goto('/CVGenerator/sync');
    await page.waitForLoadState('networkidle');
    await expect(page.getByRole('button', { name: /Export JSON/i })).toBeVisible();
    await expect(page.getByRole('button', { name: /Import JSON/i })).toBeVisible();
  });

  test('nav brand text is present', async ({ page }) => {
    await page.goto('/CVGenerator/');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('body')).toContainText(/CV Generator/i);
  });
});
