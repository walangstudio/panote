import { chromium, type FullConfig } from "playwright/test";
import { setupTauriMock } from "./mock";

// The dev server compiles the app on its first page load. Left to the first test,
// that compile ate its 15s budget on a cold or busy machine, so whichever spec ran
// first failed at random. Render the app once here, with no time limit that matters.
export default async function globalSetup(config: FullConfig) {
  const baseURL = config.projects[0].use.baseURL!;
  const browser = await chromium.launch();
  const page = await browser.newPage();
  await setupTauriMock(page);
  await page.goto(baseURL);
  await page.locator(".note-card").first().waitFor({ timeout: 120_000 });
  await page.goto(`${baseURL}/note/note-1`);
  await page.locator(".title-input").waitFor({ timeout: 120_000 });
  await browser.close();
}
