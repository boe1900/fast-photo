import { expect, test, type APIRequestContext, type Dialog, type Page } from '@playwright/test';
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
): Promise<void> {
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
}

async function loginByUi(page: Page, username: string, password: string): Promise<void> {
  await page.goto('/login');
  await page.getByLabel('用户名').fill(username);
  await page.getByLabel('密码').fill(password);
  await page.getByRole('button', { name: /登录|创建管理员/ }).click();
  await expect(page).toHaveURL(/\/$/);
}

test.describe('People Albums Settings', () => {
  test('people page shows empty state for new user', async ({ page, request }) => {
    const user = await registerUser(request, 'e2e_people');
    await createLibrary(request, user.token, `people-${Date.now()}`);
    await uploadViaApi(request, user.token, `people-${Date.now()}.png`);

    await loginByUi(page, user.username, user.password);
    await page.getByRole('button', { name: '人物' }).click();
    await expect(page).toHaveURL(/\/people$/);
    await expect(page.getByText('还没有识别到人物')).toBeVisible();
  });

  test('albums page can create and delete album', async ({ page, request }) => {
    const user = await registerUser(request, 'e2e_albums');
    await loginByUi(page, user.username, user.password);

    await page.getByRole('button', { name: '相册' }).click();
    await expect(page).toHaveURL(/\/albums$/);

    const albumName = `UI Album ${Date.now()}`;
    await page.getByRole('button', { name: '创建相册' }).click();
    await page.getByPlaceholder('输入相册名称').fill(albumName);
    await page.getByRole('button', { name: '创建', exact: true }).click();

    await expect(page.getByText(albumName)).toBeVisible({ timeout: 10000 });

    page.once('dialog', (d: Dialog) => void d.accept());
    await page.locator('.album-actions button[title="删除相册"]').first().click();
    await expect(page.getByText('还没有相册')).toBeVisible({ timeout: 10000 });
  });

  test('settings page tabs render and activity logs show tag action', async ({
    page,
    request,
  }) => {
    const user = await registerUser(request, 'e2e_settings');
    await createLibrary(request, user.token, `settings-${Date.now()}`);

    const uploadRes = await request.post(`${API}/photos/upload`, {
      headers: { Authorization: `Bearer ${user.token}` },
      multipart: {
        file: {
          name: `settings-${Date.now()}.png`,
          mimeType: 'image/png',
          buffer: SAMPLE_PNG,
        },
      },
    });
    expect(uploadRes.ok()).toBeTruthy();
    const uploadBody = await uploadRes.json();
    const photoId = uploadBody.uploaded?.[0]?.id as number;
    expect(photoId).toBeTruthy();

    const tagRes = await request.post(`${API}/photos/${photoId}/tags`, {
      headers: { Authorization: `Bearer ${user.token}` },
      data: { name: `settings-tag-${Date.now()}`, category: 'manual' },
    });
    expect(tagRes.status()).toBe(200);

    await loginByUi(page, user.username, user.password);
    await page.getByRole('button', { name: '设置' }).click();
    await expect(page).toHaveURL(/\/settings$/);

    await page.getByRole('button', { name: '存储配置' }).click();
    await expect(page.getByText('S3 兼容存储')).toBeVisible();
    await expect(page.getByRole('heading', { name: 'WebDAV' })).toBeVisible();

    await page.getByRole('button', { name: '活动日志' }).click();
    await expect(page.getByRole('heading', { name: '活动日志' })).toBeVisible();
    await expect(page.getByText('添加标签')).toBeVisible({ timeout: 10000 });
  });
});
