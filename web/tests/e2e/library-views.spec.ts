import { expect, test, type APIRequestContext, type Page } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';

const API = 'http://127.0.0.1:8080/api';
const SAMPLE_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO6pFf0AAAAASUVORK5CYII=',
  'base64'
);

interface AuthUser {
  username: string;
  password: string;
  token: string;
}

async function registerUser(request: APIRequestContext, prefix: string): Promise<AuthUser> {
  const username = `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
  const password = 'e2e-pass-123';
  const res = await request.post(`${API}/auth/register`, {
    data: { username, password },
  });
  expect(res.status()).toBe(201);
  const body = await res.json();
  return { username, password, token: body.token as string };
}

async function createLibrary(request: APIRequestContext, token: string, suffix: string): Promise<void> {
  const dir = path.join('/tmp', `fast-photo-e2e-${suffix}`);
  await fs.mkdir(dir, { recursive: true });
  const res = await request.post(`${API}/libraries`, {
    headers: { Authorization: `Bearer ${token}` },
    data: {
      name: `e2e-${suffix}`,
      path: dir,
    },
  });
  expect(res.status()).toBe(201);
}

async function uploadViaApi(
  request: APIRequestContext,
  token: string,
  fileName: string
): Promise<number> {
  const res = await request.post(`${API}/photos/upload`, {
    headers: { Authorization: `Bearer ${token}` },
    multipart: {
      file: {
        name: fileName,
        mimeType: 'image/png',
        buffer: SAMPLE_PNG,
      },
    },
  });
  expect(res.ok()).toBeTruthy();
  const body = await res.json();
  const id = body?.uploaded?.[0]?.id;
  expect(id).toBeTruthy();
  return id as number;
}

async function loginByUi(page: Page, username: string, password: string): Promise<void> {
  await page.goto('/login');
  await page.getByLabel('用户名').fill(username);
  await page.getByLabel('密码').fill(password);
  await page.getByRole('button', { name: /登录|创建管理员/ }).click();
  await expect(page).toHaveURL(/\/$/);
}

test.describe('Library Views', () => {
  test('folders page shows upload folder and photos', async ({ page, request }) => {
    const user = await registerUser(request, 'e2e_folders');
    await createLibrary(request, user.token, `folders-${Date.now()}`);
    await uploadViaApi(request, user.token, `folders-${Date.now()}.png`);

    await loginByUi(page, user.username, user.password);
    await page.getByRole('button', { name: '文件夹' }).click();
    await expect(page).toHaveURL(/\/folders$/);
    await expect(page.locator('.folder-card')).toHaveCount(1, { timeout: 10000 });
    await expect(page.getByText('Uploads')).toBeVisible();

    await page.locator('.folder-card').first().click();
    await expect(page.locator('.photo-item')).toHaveCount(1, { timeout: 10000 });
  });

  test('favorites page shows and removes favorite photo', async ({ page, request }) => {
    const user = await registerUser(request, 'e2e_fav');
    await createLibrary(request, user.token, `fav-${Date.now()}`);
    const photoId = await uploadViaApi(request, user.token, `fav-${Date.now()}.png`);

    const favRes = await request.post(`${API}/photos/${photoId}/favorite`, {
      headers: { Authorization: `Bearer ${user.token}` },
    });
    expect(favRes.status()).toBe(200);

    await loginByUi(page, user.username, user.password);
    await page.getByRole('button', { name: '收藏' }).click();
    await expect(page).toHaveURL(/\/favorites$/);
    await expect(page.locator('.photo-item')).toHaveCount(1, { timeout: 10000 });

    await page.locator('.unfav-btn').first().click();
    await expect(page.getByText('还没有收藏')).toBeVisible({ timeout: 10000 });
  });

  test('duplicates page detects duplicate and can remove copy', async ({ page, request }) => {
    const user = await registerUser(request, 'e2e_dup');
    await createLibrary(request, user.token, `dup-${Date.now()}`);
    await uploadViaApi(request, user.token, `dup-a-${Date.now()}.png`);
    await uploadViaApi(request, user.token, `dup-b-${Date.now()}.png`);

    await loginByUi(page, user.username, user.password);
    await page.getByRole('button', { name: '重复' }).click();
    await expect(page).toHaveURL(/\/duplicates$/);
    await expect(page.locator('.duplicate-group')).toHaveCount(1, { timeout: 10000 });

    await page.locator('.duplicate-delete').first().click();
    await expect(page.getByText('没有发现重复照片')).toBeVisible({ timeout: 10000 });
  });
});
