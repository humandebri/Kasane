// どこで: wrapper-vite E2E / 何を: 履歴欄の未接続表示と request route の再表示を確認する / なぜ: ローカル検証の最低限の UI 回帰を自動化するため

import { expect, test } from "@playwright/test";

test("console shows center card and connect wallet entry", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Wrap / Unwrap Console")).toBeVisible();
  await expect(page.getByRole("button", { name: "Connect Wallet" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Submit Wrap" })).toBeVisible();
  const assetRail = page.getByRole("complementary");
  await expect(assetRail.getByText("Manage Tokens")).toBeVisible();
  await expect(assetRail.getByText("ICP ICRC Tokens")).toBeVisible();
  await expect(assetRail.getByText("Internet Computer")).toBeVisible();
});

test("manage tokens drawer row click updates the current asset selector", async ({ page }) => {
  await page.goto("/");
  const assetRail = page.getByRole("complementary");
  await assetRail.getByRole("button", { name: /ckBTC/i }).click();
  await expect(page.getByRole("combobox")).toContainText("ckBTC");
});

test("wallet modal lists oisy and metamask connectors", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Connect Wallet" }).click();
  await expect(page.getByRole("heading", { name: "Connect wallet" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Connect Oisy" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Connect MetaMask" })).toBeVisible();
  await expect(page.getByText("Extension not detected in this browser.")).toBeVisible();
});

test("history route renders separate history page", async ({ page }) => {
  await page.goto("/history");
  await expect(page.locator("h1", { hasText: "Recent Requests" })).toBeVisible();
  await expect(page.getByTestId("history-panel").locator("p", { hasText: "Connect Oisy to view request history." })).toBeVisible();
});

test("request route reopens the status modal", async ({ page }) => {
  const requestId = `0x${"11".repeat(32)}`;
  await page.goto(`/requests/${requestId}`);
  await expect(page.getByText("Request Status")).toBeVisible();
  await expect(page.getByText(requestId)).toBeVisible();
});
