import { test, expect } from '@playwright/test';
import { clearStorage } from './helpers';

test.describe('SPA Navigation', () => {

  test.beforeEach(async ({ page }) => {
    await page.goto('/CVGenerator/');
    await page.waitForLoadState('networkidle');
  });

  test('landing page loads', async ({ page }) => {
    await expect(page.locator('body')).toContainText(/CV Generator/i);
  });

  test('nav bar renders with links', async ({ page }) => {
    await expect(page.locator('header.nav')).toBeVisible();
    await expect(page.locator('.nav-links')).toBeVisible();
    const links = page.locator('.nav-links a');
    await expect(links).toHaveCount(5);
  });

  test('nav links have correct routes', async ({ page }) => {
    const links = page.locator('.nav-links a');
    await expect(links.nth(0)).toHaveAttribute('href', /CVGenerator\/?$/);
    await expect(links.nth(1)).toHaveAttribute('href', /cv\/edit/);
    await expect(links.nth(2)).toHaveAttribute('href', /cv\/preview/);
    await expect(links.nth(3)).toHaveAttribute('href', /tailor/);
    await expect(links.nth(4)).toHaveAttribute('href', /sync/);
  });

  test('click Sync nav goes to /sync', async ({ page }) => {
    await page.locator('.nav-links a').nth(4).click();
    await expect(page).toHaveURL(/\/sync/);
  });

  test('nav brand is clickable', async ({ page }) => {
    await page.goto('/CVGenerator/sync');
    await page.waitForLoadState('networkidle');
    await page.locator('.nav-brand').click();
    await expect(page).toHaveURL(/\/CVGenerator\/?$/);
  });

  test('nav toggles render', async ({ page }) => {
    const toggles = page.locator('.nav-toggle');
    await expect(toggles).toHaveCount(2);
  });

  test('/cv/edit loads', async ({ page }) => {
    await page.goto('/CVGenerator/cv/edit');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h1')).toBeVisible();
  });

  test('/cv/preview loads', async ({ page }) => {
    await page.goto('/CVGenerator/cv/preview');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h1')).toBeVisible();
  });

  test('/tailor loads', async ({ page }) => {
    await page.goto('/CVGenerator/tailor');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h1')).toBeVisible();
  });

  test('/sync loads', async ({ page }) => {
    await page.goto('/CVGenerator/sync');
    await page.waitForLoadState('networkidle');
    await expect(page.getByRole('heading', { name: /Google Drive/i })).toBeVisible();
    await expect(page.getByRole('heading', { name: /Local Backup/i })).toBeVisible();
  });

  test('404 edges return app shell', async ({ page }) => {
    await page.goto('/CVGenerator/nonexistent');
    await page.waitForLoadState('networkidle');
    const title = await page.title();
    expect(title.length).toBeGreaterThan(0);
  });
});
