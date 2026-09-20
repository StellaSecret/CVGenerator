import { test, expect, type Page } from '@playwright/test';
import { PDFDocument } from 'pdf-lib';

// A small but realistic LifetimeCV so the preview iframe has enough content
// to exceed the print-fit minimum height (100mm) at A4 width.
const SEED_CV = JSON.stringify({
  personal: {
    name: 'Jane Smith',
    title: { en: 'Senior Software Engineer', fr: 'Ingénieure logiciel senior' },
    email: 'jane@example.com',
    phone: '+33 6 12 34 56 78',
    location: 'Paris, France',
    linkedin: 'linkedin.com/in/janesmith',
    github: 'github.com/janesmith',
    website: 'janesmith.dev',
    summary: {
      en: 'Engineer who ships production software on a deadline. Cuts through ambiguity fast and leaves systems easier to run than they were found. Ten years of experience across distributed systems, developer tooling, and cloud infrastructure.',
      fr: 'Ingénieure qui livre du logiciel en production dans les délais. Tranche vite dans l\'ambiguïté et laisse les systèmes plus simples à exploiter qu\'ils n\'étaient. Dix ans d\'expérience dans les systèmes distribués, l\'outillage développeur et l\'infrastructure cloud.',
    },
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
          context: [{ en: 'Led a 6-person team.', fr: 'Dirigé une équipe de 6 personnes.' }],
          bullets: [
            { en: 'Cut p95 latency from 2s to 200ms by replacing the batch pipeline with a streaming one.', fr: 'Réduit la latence p95 de 2s à 200ms en remplaçant le pipeline batch par un pipeline streaming.' },
            { en: 'Introduced trunk-based development; release time dropped from weeks to daily.', fr: 'Introduit le développement trunk-based ; le temps de release est passé de semaines à quotidien.' },
            { en: 'Mentored three engineers to senior level.', fr: 'Mentoré trois ingénieurs au niveau senior.' },
          ],
          skill_ids: ['s-rust', 's-kubernetes'],
          start_date: 'Jan 2020',
          end_date: 'Jun 2023',
        },
        {
          id: 'proj-2',
          name: { en: 'Developer tooling', fr: 'Outillage développeur' },
          context: [],
          bullets: [{ en: 'Built an internal CLI adopted by 120+ engineers.', fr: 'Construit un CLI interne adopté par plus de 120 ingénieurs.' }],
          skill_ids: [],
          start_date: '',
          end_date: '',
        },
      ],
    },
  ],
  skills: [
    { id: 's-rust', name: 'Rust', category: 'Programming', level: 'Advanced' },
    { id: 's-kubernetes', name: 'Kubernetes', category: 'PlatformsInfrastructure', level: 'Expert' },
    { id: 's-aws', name: 'AWS', category: 'PlatformsInfrastructure', level: 'Advanced' },
  ],
  education: [
    {
      id: 'edu-1',
      institution: 'University of Example',
      degree: { en: 'MSc', fr: 'Master' },
      field: { en: 'Computer Science', fr: 'Informatique' },
      start_year: '2009',
      end_year: '2013',
      achievements: [],
    },
  ],
  projects: [
    {
      id: 'p-1',
      name: 'Open Source Library',
      description: { en: 'A small library that ships useful things.', fr: '' },
      url: 'github.com/janesmith/lib',
      tools: ['Rust', 'WASM'],
      bullets: [{ en: '2k GitHub stars.', fr: '' }],
    },
  ],
  languages: [
    { id: 'l-1', name: 'English', level: 'Native' },
    { id: 'l-2', name: 'French', level: 'Native' },
  ],
  certifications: [
    { id: 'c-1', name: 'AWS Certified Solutions Architect', issuer: 'Amazon', date: '2021', url: '' },
  ],
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

test.describe('print-to-PDF', () => {
  test('downloads a single long scrollable page named after the candidate', async ({ page }) => {
    await seedCV(page);
    await page.goto('/CVGenerator/cv/preview');
    await page.waitForLoadState('networkidle');

    const frameLocator = page.frameLocator('#cv-preview-frame');
    await expect(frameLocator.locator('.cv-doc')).toBeVisible();

    const handle = await page.locator('#cv-preview-frame').elementHandle();
    const frame = (await handle?.contentFrame()) ?? null;
    expect(frame).not.toBeNull();

    // Stub the iframe's print() so no native dialog opens; record everything
    // the app's download_pdf_js did just before calling it.
    await frame!.evaluate(() => {
      const w = window as unknown as {
        __printInfo: unknown;
        print: () => void;
      };
      w.__printInfo = null;
      w.print = () => {
        const s = document.getElementById('print-fit');
        let pageMm: number | null = null;
        if (s) {
          const m = /@page\{size:210mm (\d+)mm;margin:0\}/.exec(s.textContent ?? '');
          pageMm = m ? parseInt(m[1], 10) : null;
        }
        // Mirror download_pdf_js's own measurement (.cv-doc's bounding
        // box), not documentElement/body — those were found running
        // ~3000px taller than the real content in some cases, for
        // reasons unrelated to .cv-doc itself, which silently padded
        // every exported PDF with that much blank trailing space.
        const cvDocEl = document.querySelector('.cv-doc');
        const measuredPx = cvDocEl
          ? cvDocEl.getBoundingClientRect().height
          : document.documentElement.scrollHeight;
        w.__printInfo = {
          title: document.title,
          pageMm,
          measuredPx,
        };
      };
    });

    await page.getByRole('button', { name: /Download PDF/i }).click();

    await expect.poll(() => frame!.evaluate(() => (window as unknown as { __printInfo: unknown }).__printInfo)).not.toBeNull();
    const info = await frame!.evaluate(() =>
      (window as unknown as { __printInfo: { title: string; pageMm: number; measuredPx: number } }).__printInfo,
    );

    // Filename behavior: document.title carries the suggested save-as name
    // (extension stripped; the browser appends .pdf).
    expect(info.title).toBe('jane-smith-cv');

    // One page sized to fit the content: measured height plus the safety
    // padding (print rendering can run a couple of px taller than screen).
    const paddedMm = (measuredPx: number) =>
      Math.ceil(Math.ceil((measuredPx * 25.4) / 96) * 1.01) + 1;
    expect(info.pageMm).toBeGreaterThan(100);
    expect(info.pageMm).toBeLessThanOrEqual(5000);
    expect(info.pageMm).toBe(paddedMm(info.measuredPx));

    // Prove a genuine single-tall-page PDF: replay the same print-fit
    // injection on the iframe's srcdoc and export it with the real engine.
    //
    // Passing explicit width/height to page.pdf() was tried and, twice,
    // still produced 2 pages even when the height was byte-for-byte the
    // same value download_pdf_js computed — so the API's own width/height
    // options aren't a reliable stand-in for what the real flow does.
    // That's because the real flow never calls the pdf() API at all: it
    // sets a literal `@page{size:...;margin:0}` rule and calls the
    // browser's native window.print(). page.pdf()'s width/height options
    // go through a separate CDP paperWidth/paperHeight + margin code path
    // that apparently doesn't guarantee margin:0 the same way CSS @page
    // does here. So instead of approximating the production behavior via
    // API options, this replays the *exact* mechanism: inject the same
    // @page{size:210mm <pageMm>mm;margin:0} rule the app would inject,
    // and let preferCSSPageSize hand page sizing to that CSS rule rather
    // than to page.pdf()'s own width/height parameters.
    const srcdoc = await page
      .locator('#cv-preview-frame')
      .evaluate((el) => el.getAttribute('srcdoc') ?? '');
    const pdfPage = await page.context().newPage();
    await pdfPage.setContent(srcdoc, { waitUntil: 'load' });
    await pdfPage.emulateMedia({ media: 'print' });
    const heightMm = info.pageMm;
    await pdfPage.evaluate((pageMm) => {
      const style = document.createElement('style');
      style.id = 'print-fit';
      style.textContent =
        `@page{size:210mm ${pageMm}mm;margin:0}` +
        'html,body{margin:0!important}' +
        '.cv-doc .toolbar,.cv-doc .gap-banner{display:none!important}' +
        // Fixed width (not max-width) — see download_pdf_js in
        // src/views/mod.rs. A ceiling alone lets .cv-doc get squeezed
        // narrower than A4 by whatever on-screen container the srcdoc
        // happens to render in, which is exactly what let the bug through:
        // both info.pageMm here and the real download_pdf_js measurement
        // must use the identical fixed-width rule or this replay stops
        // being a faithful test of production behavior.
        '.cv-doc{width:754px;max-width:none;padding:20px!important}';
      document.head.appendChild(style);
    }, heightMm);
    const pdf = await pdfPage.pdf({ printBackground: true, preferCSSPageSize: true });
    await pdfPage.close();

    expect(pdf.subarray(0, 5).toString()).toBe('%PDF-');
    const doc = await PDFDocument.load(pdf);
    expect(doc.getPageCount()).toBe(1, 'one long page, no per-sheet breaks');
    const [sheet] = doc.getPages();
    const { width, height } = sheet.getSize();
    // 210mm A4 width ≈ 595.3pt.
    expect(width).toBeGreaterThan(590);
    expect(width).toBeLessThan(600);
    // Height matches the padded content height (± a couple of points).
    const expectedPt = (heightMm * 72) / 25.4;
    expect(height).toBeGreaterThan(expectedPt - 5);
    expect(height).toBeLessThan(expectedPt + 5);
  });
});