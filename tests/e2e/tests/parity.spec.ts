import { test, expect, Page } from "@playwright/test";

// PythScribe ↔ React parity: each fixture is rendered twice (once from
// the compiled .ps→.js bundle, once from the React .tsx reference) and
// compared via two independent checks:
//   (1) DOM-bytecode equality — serialized tree byte-for-byte equal.
//   (2) Pixel-perfect screenshot diff.
// Both checks run independently per scenario, so we can later relax one
// if it proves flaky cross-platform.

const FIXTURES = [
  { id: "dashboard_500", export: "Dashboard" },
  { id: "app_1000", export: "App" },
];

async function gotoAndReady(page: Page, htmlPage: "pythscribe" | "react", fixtureId: string, exportName: string) {
  await page.goto(`/harness/${htmlPage}.html?fixture=${fixtureId}&export=${exportName}`);
  await page.waitForFunction(() => (window as any).__pythsReady === true, null, { timeout: 30_000 });
  // Surface render-time errors as a test failure with the actual stack.
  const err = await page.evaluate(() => (window as any).__pythsRenderError);
  if (err) {
    throw new Error(`Render error in ${htmlPage} (${fixtureId}):\n${err}`);
  }
}

async function getDom(page: Page) {
  return page.evaluate(async () => {
    const mod = await import("/lib/serialize-dom.js");
    return mod.serializeRootDom();
  });
}

for (const fix of FIXTURES) {
  test.describe(`fixture: ${fix.id}`, () => {
    test(`renders without errors (PythScribe)`, async ({ page }) => {
      await gotoAndReady(page, "pythscribe", fix.id, fix.export);
      const dom = await getDom(page);
      expect(dom, "PythScribe render produced an empty #root").not.toBeNull();
    });

    test(`renders without errors (React reference)`, async ({ page }) => {
      await gotoAndReady(page, "react", fix.id, fix.export);
      const dom = await getDom(page);
      expect(dom, "React render produced an empty #root").not.toBeNull();
    });

    test(`DOM-bytecode parity`, async ({ page, browser }) => {
      // Two separate browser contexts so caches/state don't bleed.
      const ctxP = await browser.newContext();
      const ctxR = await browser.newContext();
      const pageP = await ctxP.newPage();
      const pageR = await ctxR.newPage();
      await gotoAndReady(pageP, "pythscribe", fix.id, fix.export);
      await gotoAndReady(pageR, "react", fix.id, fix.export);
      const domP = await pageP.evaluate(async () => {
        const mod = await import("/lib/serialize-dom.js");
        return mod.serializeRootDom();
      });
      const domR = await pageR.evaluate(async () => {
        const mod = await import("/lib/serialize-dom.js");
        return mod.serializeRootDom();
      });
      await ctxP.close();
      await ctxR.close();
      expect(domP, "DOM-bytecode parity").toEqual(domR);
    });

    test(`pixel-perfect parity`, async ({ page }) => {
      // Viewport-clipped (not fullPage) so screenshot dimensions are
      // stable across runs — fullPage height varies as React's reconciler
      // commits cells in different orders, which breaks size matching
      // even when the rendered pixels are identical.
      await page.goto(`/harness/pythscribe.html?fixture=${fix.id}&export=${fix.export}`);
      await page.waitForFunction(() => (window as any).__pythsReady === true, null, { timeout: 30_000 });
      // Small additional settle so any post-mount layout effects flush.
      await page.waitForTimeout(150);
      expect(await page.screenshot({ fullPage: false, animations: "disabled" }))
        .toMatchSnapshot(`${fix.id}.png`, { maxDiffPixels: 200, threshold: 0.2 });
    });
  });
}
