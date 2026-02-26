import { expect, test } from '@playwright/test';

test.describe('Auth and Navigation', () => {
  test('login with seeded user and navigate core pages', async ({ page, request }) => {
    const username = `e2e_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const password = 'e2e-pass-123';

    // Registration endpoint is open; seed an account for deterministic login.
    const reg = await request.post('http://127.0.0.1:8080/api/auth/register', {
      data: { username, password },
    });
    expect([201, 409]).toContain(reg.status());

    await page.goto('/login');
    await page.getByLabel('用户名').fill(username);
    await page.getByLabel('密码').fill(password);
    await page.getByRole('button', { name: /登录|创建管理员/ }).click();

    await expect(page).toHaveURL(/\/$/);
    await expect(page.locator('h1', { hasText: '照片' })).toBeVisible();

    await page.getByRole('button', { name: '设置' }).click();
    await expect(page.getByRole('heading', { name: '设置' })).toBeVisible();

    await page.getByRole('button', { name: '收藏' }).click();
    await expect(page.getByRole('heading', { name: '收藏' })).toBeVisible();
  });

  test('theme switch persists after reload', async ({ page, request }) => {
    const username = `e2e_theme_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const password = 'e2e-pass-123';

    const reg = await request.post('http://127.0.0.1:8080/api/auth/register', {
      data: { username, password },
    });
    expect([201, 409]).toContain(reg.status());

    await page.goto('/login');
    await page.getByLabel('用户名').fill(username);
    await page.getByLabel('密码').fill(password);
    await page.getByRole('button', { name: /登录|创建管理员/ }).click();
    await expect(page).toHaveURL(/\/$/);

    const html = page.locator('html');
    const before = await html.getAttribute('data-theme');
    await page.locator('.theme-toggle').click();
    const after = await html.getAttribute('data-theme');

    expect(after).not.toBe(before);
    await page.reload();
    await expect(html).toHaveAttribute('data-theme', after || '');
  });
});
