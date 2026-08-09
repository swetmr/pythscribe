import { test, expect, Page } from "@playwright/test";
import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";

// Load the manifest of generated fuzz fixtures. Each entry was produced
// by tests/e2e/fuzz/generate.mjs from a JSON spec — both the .ps and
// .tsx files are emitted to render structurally identical DOMs.
//
// This suite proves that across the whole spec corpus.

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const MANIFEST_PATH = path.join(__dirname, "..", "fuzz", "generated", "manifest.json");

type ManifestEntry = { id: string; exportName: string };

let manifest: ManifestEntry[] = [];
try {
  const raw = await fs.readFile(MANIFEST_PATH, "utf8");
  manifest = JSON.parse(raw);
} catch {
  // No manifest yet — generator hasn't been run. Suite will report 0 tests.
}

async function gotoAndReady(page: Page, kind: "fuzz-pythscribe" | "fuzz-react", id: string) {
  await page.goto(`/harness/${kind}.html?id=${id}`);
  // esbuild-wasm initialization on first cold cache is slow.
  await page.waitForFunction(() => (window as any).__pythsReady === true, null, { timeout: 50_000 });
  const err = await page.evaluate(() => (window as any).__pythsRenderError);
  if (err) throw new Error(`Render error in ${kind} (${id}):\n${err}`);
}

async function getDom(page: Page) {
  return page.evaluate(async () => {
    const mod = await import("/lib/serialize-dom.js");
    return mod.serializeRootDom();
  });
}

for (const entry of manifest) {
  test.describe(`fuzz fixture: ${entry.id}`, () => {
    test(`renders without errors (PythScribe)`, async ({ page }) => {
      await gotoAndReady(page, "fuzz-pythscribe", entry.id);
      expect(await getDom(page)).not.toBeNull();
    });

    test(`renders without errors (React reference)`, async ({ page }) => {
      await gotoAndReady(page, "fuzz-react", entry.id);
      expect(await getDom(page)).not.toBeNull();
    });

    test(`DOM-bytecode parity`, async ({ browser }) => {
      const ctxP = await browser.newContext();
      const ctxR = await browser.newContext();
      const pageP = await ctxP.newPage();
      const pageR = await ctxR.newPage();
      await gotoAndReady(pageP, "fuzz-pythscribe", entry.id);
      await gotoAndReady(pageR, "fuzz-react", entry.id);
      const domP = await getDom(pageP);
      const domR = await getDom(pageR);
      await ctxP.close();
      await ctxR.close();
      expect(domP, `DOM-bytecode parity for ${entry.id}`).toEqual(domR);
    });

    test(`pixel-parity vs React reference`, async ({ browser }) => {
      // Render both pages in fresh contexts so caches/state don't bleed,
      // capture viewport-clipped (not fullPage — heights vary by content
      // and cause spurious size mismatches) screenshots, then pixel-diff
      // via pixelmatch. No stored baselines: this is a live A/B between
      // PythScribe-rendered and React-rendered output for each generated
      // fuzz fixture, so a tighter signal than "this matches my old
      // snapshot".
      const ctxP = await browser.newContext({ viewport: { width: 1280, height: 800 } });
      const ctxR = await browser.newContext({ viewport: { width: 1280, height: 800 } });
      const pageP = await ctxP.newPage();
      const pageR = await ctxR.newPage();
      await gotoAndReady(pageP, "fuzz-pythscribe", entry.id);
      await gotoAndReady(pageR, "fuzz-react", entry.id);
      // Tiny settle so any post-mount layout effects flush.
      await pageP.waitForTimeout(50);
      await pageR.waitForTimeout(50);
      const pngP = await pageP.screenshot({ fullPage: false, animations: "disabled" });
      const pngR = await pageR.screenshot({ fullPage: false, animations: "disabled" });
      await ctxP.close();
      await ctxR.close();

      const imgP = PNG.sync.read(pngP);
      const imgR = PNG.sync.read(pngR);
      expect(imgP.width, `width mismatch for ${entry.id}`).toBe(imgR.width);
      expect(imgP.height, `height mismatch for ${entry.id}`).toBe(imgR.height);

      const diff = new PNG({ width: imgP.width, height: imgP.height });
      const diffPixels = pixelmatch(
        imgP.data, imgR.data, diff.data,
        imgP.width, imgP.height,
        // threshold: per-pixel color delta tolerance (0..1). 0.1 is
        // tight; AA differences at ~0.05 typically pass.
        { threshold: 0.1 },
      );

      // Anchor the diff budget on image area: at most 0.5% of pixels
      // may differ. For a 1280×800 viewport that's ~5,120 pixels.
      const total = imgP.width * imgP.height;
      const budget = Math.max(50, Math.floor(total * 0.005));

      if (diffPixels > budget) {
        // Persist the actual + expected + diff PNGs so failures are
        // triageable from CI artifacts.
        const outDir = path.resolve(
          path.join(__dirname, "..", "test-results", `fuzz-pixel-${entry.id}`)
        );
        await fs.mkdir(outDir, { recursive: true });
        await fs.writeFile(path.join(outDir, "pythscribe.png"), pngP);
        await fs.writeFile(path.join(outDir, "react.png"), pngR);
        await fs.writeFile(path.join(outDir, "diff.png"), PNG.sync.write(diff));
        throw new Error(
          `pixel parity ${entry.id}: ${diffPixels} diffs (budget ${budget}, total ${total}). ` +
          `See ${outDir}`
        );
      }
    });
  });
}
