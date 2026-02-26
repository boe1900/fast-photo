import { expect, test, type APIRequestContext, type Page } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';

const API = 'http://127.0.0.1:8080/api';
const SAMPLE_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR42mP4z8AAAAMBAQD3A0FDAAAAAElFTkSuQmCC',
  'base64'
);

interface AuthUser {
  username: string;
  password: string;
  token: string;
}

interface TimelinePhoto {
  id: number;
  file_name: string;
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

async function createLibrary(request: APIRequestContext, token: string, suffix: string): Promise<string> {
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
  return dir;
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
  if (body?.uploaded?.[0]?.id) {
    return body.uploaded[0].id as number;
  }

  const timeline = await request.get(`${API}/photos/timeline`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  expect(timeline.status()).toBe(200);
  const tl = (await timeline.json()) as { data?: TimelinePhoto[] };
  const found = (tl.data || []).find((p) => p.file_name === fileName) || tl.data?.[0];
  expect(found?.id).toBeTruthy();
  return found.id as number;
}

async function loginByUi(page: Page, username: string, password: string): Promise<void> {
  await page.goto('/login');
  await page.getByLabel('用户名').fill(username);
  await page.getByLabel('密码').fill(password);
  await page.getByRole('button', { name: /登录|创建管理员/ }).click();
  await expect(page).toHaveURL(/\/$/);
}

test.describe('Map and Explore', () => {
  test('map page shows empty state when user has no geo photos', async ({ page, request }) => {
    const user = await registerUser(request, 'e2e_map_empty');
    await createLibrary(request, user.token, `map-empty-${Date.now()}`);
    await uploadViaApi(request, user.token, `map-empty-${Date.now()}.png`);

    await loginByUi(page, user.username, user.password);
    await page.getByRole('button', { name: '地图' }).click();

    await expect(page).toHaveURL(/\/map$/);
    await expect(page.locator('h1', { hasText: '地图' })).toBeVisible();
    await expect(page.getByText('没有带定位的照片')).toBeVisible();
  });

  test('explore page renders scoped tags and ai action opens dialog', async ({ page, request }) => {
    test.setTimeout(90_000);

    const user = await registerUser(request, 'e2e_explore');
    await createLibrary(request, user.token, `explore-${Date.now()}`);
    const photoId = await uploadViaApi(request, user.token, `explore-${Date.now()}.png`);
    const tagName = `manual-tag-${Date.now()}`;

    const tagRes = await request.post(`${API}/photos/${photoId}/tags`, {
      headers: { Authorization: `Bearer ${user.token}` },
      data: { name: tagName, category: 'manual' },
    });
    expect(tagRes.status()).toBe(200);

    await loginByUi(page, user.username, user.password);
    await page.getByRole('button', { name: '发现' }).click();
    await expect(page).toHaveURL(/\/explore$/);
    await expect(page.locator('h1', { hasText: '智能发现' })).toBeVisible();
    await expect(page.getByRole('button', { name: new RegExp(tagName) })).toBeVisible();

    const [processResponse, dialog] = await Promise.all([
      page.waitForResponse((resp) => {
        return resp.url().includes('/api/ai/process') && resp.request().method() === 'POST';
      }),
      page.waitForEvent('dialog', { timeout: 45_000 }),
      page.getByRole('button', { name: 'AI 分析照片' }).click(),
    ]);
    expect(processResponse.ok()).toBeTruthy();
    expect(dialog.message().length).toBeGreaterThan(0);
    await dialog.accept();
  });
});
