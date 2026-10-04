import { type Page } from '@playwright/test';

export async function clearStorage(page: Page) {
  await page.goto('/CVGenerator/app/');
  await page.waitForLoadState('networkidle');
  await page.evaluate(() => {
    localStorage.clear();
  });
  await page.reload();
  await page.waitForLoadState('networkidle');
}
