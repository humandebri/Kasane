import assert from "node:assert/strict";
import { chromium } from "playwright";

const baseUrl = process.env.EXPLORER_TEST_BASE_URL;
if (!baseUrl) throw new Error("EXPLORER_TEST_BASE_URL is required (a local test server)");
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, reducedMotion: "reduce" });
  page.setDefaultTimeout(15000);
  const errors: string[] = [];
  page.on("response", (response) => { if (response.url().startsWith(baseUrl) && response.status() >= 500) errors.push(`HTTP ${response.status()}: ${new URL(response.url()).pathname}`); });
  page.on("pageerror", (error) => errors.push(`${new URL(page.url()).pathname}: ${error.message}`));
  page.on("console", (message) => { if (message.type() === "error" && /hydration|Buffer is not defined|process is not defined/i.test(message.text())) errors.push(message.text()); });
  const routes = ["/verify", "/blocks", "/txs", "/ops", "/logs?fromBlock=0&toBlock=0", "/address/0x" + "11".repeat(20)];
  if (process.env.EXPLORER_TEST_RPC === "1") routes.push("/", "/blocks/12", "/tx/0x" + "aa".repeat(32));
  for (const route of routes) {
    const response = await page.goto(baseUrl + route);
    assert.equal(response?.status(), 200, route);
    await page.waitForLoadState("networkidle");
    assert.ok(await page.locator("main").innerText(), route);
    if (route === "/txs") assert.ok(await page.getByText("transfer", { exact: true }).first().isVisible());
    if (route === "/blocks/12") assert.ok(await page.getByText("Block 12", { exact: true }).isVisible());
    console.log(`page passed: ${route}`);
  }
  await page.goto(baseUrl + "/address/0x" + "11".repeat(20));
  await page.getByRole("link", { name: "Token Transfers (ERC-20)", exact: true }).click();
  await page.waitForURL(/tab=token/);
  await page.getByRole("link", { name: "Contract Events", exact: true }).click();
  await page.waitForURL(/tab=events/);
  await page.goBack();
  await page.waitForURL(/tab=token/);
  await page.reload();
  await page.waitForLoadState("networkidle");
  await page.goto(baseUrl + "/logs?fromBlock=0&toBlock=0");
  await page.waitForLoadState("networkidle");
  await page.locator('input[name="fromBlock"]').fill("1");
  await page.locator('input[name="fromBlock"]').press("Enter");
  await page.waitForURL(/fromBlock=1/);
  assert.equal(new URL(page.url()).searchParams.get("toBlock"), "0");
  await page.goto(baseUrl + "/verify");
  await page.waitForLoadState("networkidle");
  await page.locator('input[name="q"]').fill("0x" + "11".repeat(20));
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await page.waitForURL(/\/address\/0x11/);
  for (const path of ["/does-not-exist", "/address/not-hex", "/principal/not-a-principal"]) {
    const response = await page.goto(baseUrl + path);
    assert.equal(response?.status(), 404, path);
  }
  const principalRedirect = await page.request.get(baseUrl + "/principal/nggqm-p5ozz-i5hfv-bejmq-2gtow-4dtqw-vjatn-4b4yw-s5mzs-i46su-6ae", { maxRedirects: 0 });
  assert.equal(principalRedirect.status(), 307);
  assert.match(principalRedirect.headers()["location"] ?? "", /address\/0xf53e047376e37eac56d48245b725c47410cf6f1e/);
  const blockSearch = await page.request.get(baseUrl + "/search?q=00012", { maxRedirects: 0 });
  assert.equal(blockSearch.status(), 307);
  assert.match(blockSearch.headers()["location"] ?? "", /blocks\/00012$/);
  const lookup = await page.request.get(baseUrl + "/api/contracts/invalid/verified");
  assert.equal(lookup.status(), 400);
  assert.deepEqual(await lookup.json(), { error: "invalid address" });
  for (const method of ["get", "post"] as const) {
    const path = method === "get" ? "/api/verify/status?id=test" : "/api/verify/submit";
    const response = await page.request[method](baseUrl + path);
    assert.ok([401, 503].includes(response.status()), path);
  }
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto(baseUrl + "/verify");
  assert.ok(await page.getByRole("heading", { name: "Contract Verify" }).isVisible());
  assert.deepEqual(errors, []);
  console.log(`browser smoke passed (${routes.length} pages, navigation, API, 404, mobile)`);
} finally {
  await browser.close();
}
