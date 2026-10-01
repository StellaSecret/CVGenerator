import { test, expect, type Page } from '@playwright/test';

// Minimal but complete enough to satisfy the tailor view's `has_cv` gate
// (which only checks `personal.name`) while giving the saved-sessions panel
// something to act on.
const SEED_CV = JSON.stringify({
  personal: {
    name: 'Jane Smith',
    title: { en: 'Senior Software Engineer', fr: 'Ingénieure logiciel senior' },
    email: 'jane@example.com',
    phone: '',
    location: 'Paris, France',
    linkedin: '',
    github: '',
    website: '',
    summary: { en: 'Engineer who ships.', fr: 'Ingénieure qui livre.' },
  },
  experiences: [
    {
      id: 'exp-1',
      company: 'Example Corp',
      role: { en: 'Senior Software Engineer', fr: 'Ingénieure logiciel senior' },
      location: 'Paris, France',
      start_date: 'Jan 2020',
      end_date: 'Present',
      projects: [
        {
          id: 'proj-1',
          name: { en: 'Platform rewrite', fr: 'Réécriture de la plateforme' },
          context: [],
          bullets: [{ en: 'Cut p95 latency from 2s to 200ms.', fr: '' }],
          skill_ids: ['s-rust'],
          start_date: '',
          end_date: '',
        },
      ],
    },
  ],
  skills: [{ id: 's-rust', name: 'Rust', category: 'Programming', level: 'Advanced' }],
  education: [],
  projects: [],
  languages: [],
  certifications: [],
});

async function seedCV(page: Page) {
  await page.goto('/CVGenerator/');
  await page.waitForLoadState('networkidle');
  await page.evaluate(
    (cv) => localStorage.setItem('cv_generator_lifetime_cv', cv),
    SEED_CV,
  );
  await page.reload();
  await page.waitForLoadState('networkidle');
}

test.describe('saved-sessions panel', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/CVGenerator/');
    await page.waitForLoadState('networkidle');
    await page.evaluate(() => {
      localStorage.clear();
      // Pin the language: `Lang::detect()` falls back to navigator.language,
      // so without this the assertions below would pass or fail depending on
      // the CI browser's locale rather than on the behaviour under test.
      localStorage.setItem('cv_gen_lang', 'en');
    });
    await seedCV(page);
    await page.goto('/CVGenerator/tailor');
    await page.waitForLoadState('networkidle');
  });

  test('the name field explains why the save button starts disabled', async ({ page }) => {
    const input = page.locator('#save-session-name-input');
    const button = page.getByRole('button', { name: /save as/i });
    const hint = page.locator('#save-session-name-hint');

    await expect(input).toBeVisible();
    // The button is gated on the name, so the name has to be labelled and
    // the requirement stated: a permanently greyed button with no visible
    // explanation reads as a broken control.
    await expect(button).toBeDisabled();
    await expect(hint).toHaveText(/enter a name/i);
    // The hint is wired to the field it describes, not just present.
    await expect(input).toHaveAttribute('aria-describedby', 'save-session-name-hint');
  });

  test('typing a name enables saving and adds the session', async ({ page }) => {
    const input = page.locator('#save-session-name-input');
    const button = page.getByRole('button', { name: /save as/i });

    await expect(button).toBeDisabled();
    await input.fill('Acme — Platform Engineer');
    await expect(button).toBeEnabled();

    await button.click();

    await expect(page.locator('.saved-session-name')).toHaveText('Acme — Platform Engineer');
    // Saved to storage, not just rendered.
    const stored = await page.evaluate(() =>
      localStorage.getItem('cv_generator_saved_sessions'),
    );
    expect(stored).toContain('Acme — Platform Engineer');
  });

  test('loading a session switches saving to an in-place update', async ({ page }) => {
    const input = page.locator('#save-session-name-input');

    await input.fill('Acme — Platform Engineer');
    await page.getByRole('button', { name: /save as/i }).click();
    await expect(page.locator('.saved-session-name')).toHaveText('Acme — Platform Engineer');

    await page.getByRole('button', { name: /^load$/i }).click();

    // Saving now updates the loaded session instead of minting a copy, and the
    // name comes back with it so renaming is a plain edit of the prefilled value.
    const updateButton = page.getByRole('button', { name: /^update$/i });
    await expect(updateButton).toBeVisible();
    await expect(input).toHaveValue('Acme — Platform Engineer');
    await expect(page.getByRole('button', { name: /save as new/i })).toBeVisible();

    await input.fill('Acme — Staff Engineer');
    await updateButton.click();

    // One row, renamed: an in-place update rather than a second entry.
    await expect(page.locator('.saved-session-name')).toHaveCount(1);
    await expect(page.locator('.saved-session-name')).toHaveText('Acme — Staff Engineer');
    const stored = await page.evaluate(() =>
      localStorage.getItem('cv_generator_saved_sessions'),
    );
    expect(stored).toContain('Acme — Staff Engineer');
    expect(stored).not.toContain('Acme — Platform Engineer');
  });

  test('whitespace alone does not enable saving', async ({ page }) => {
    const input = page.locator('#save-session-name-input');
    const button = page.getByRole('button', { name: /save as/i });

    await input.fill('   ');
    await expect(button).toBeDisabled();
  });
});
