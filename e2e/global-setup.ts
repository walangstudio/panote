import { chromium, type FullConfig } from "playwright/test";
import { setupTauriMock } from "./mock";
import { writeFakeCamera } from "./optical-fixture";

// The dev server compiles the app on its first page load. Left to the first test,
// that compile ate its 15s budget on a cold or busy machine, so whichever spec ran
// first failed at random. Render the app once here, with no time limit that matters.
export default async function globalSetup(config: FullConfig) {
  // Before any browser launches: the camera spec hands this file to Chromium.
  await writeFakeCamera();
  const baseURL = config.projects[0].use.baseURL!;
  const browser = await chromium.launch();
  const page = await browser.newPage();
  await setupTauriMock(page);
  await page.goto(baseURL);
  await page.locator(".note-card").first().waitFor({ timeout: 120_000 });
  // Every editor kind: each pulls in its own dependencies (TipTap for documents),
  // and the first spec to open one would otherwise pay for that compile.
  for (const kind of ["document", "checklist", "kanban", "table"]) {
    await page.goto(`${baseURL}/note/new?kind=${kind}`);
    await page.locator(".title-input").waitFor({ timeout: 120_000 });
  }
  await browser.close();
}
