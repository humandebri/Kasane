// どこで: wrapper-vite E2E / 何を: 履歴欄の未接続表示と request route の再表示を確認する / なぜ: ローカル検証の最低限の UI 回帰を自動化するため

import { expect, test } from "@playwright/test";

test("disconnected history shows connect-required message", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Recent Requests" })).toBeVisible();
  await expect(page.getByText("Connect wallet to load history").first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Continue with Google" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Internet Identity" })).toBeVisible();
});

test("request route reopens the status modal", async ({ page }) => {
  const requestId = `0x${"11".repeat(32)}`;
  await page.goto(`/requests/${requestId}`);
  await expect(page.getByText("Request Status")).toBeVisible();
  await expect(page.getByText(requestId)).toBeVisible();
});

test("google callback route renders completion state", async ({ page }) => {
  await page.goto("/auth/callback");
  await expect(page.getByText("Completing sign-in...")).toBeVisible();
});
