import { expect, test, type APIRequestContext } from '@playwright/test';
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

interface TimelinePhoto {
  id: number;
  file_name: string;
}

interface UploadResult {
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
): Promise<UploadResult> {
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
    return {
      id: body.uploaded[0].id as number,
      file_name: (body.uploaded[0].file_name as string) || fileName,
    };
  }

  const timeline = await request.get(`${API}/photos/timeline`, {
    headers: { Authorization: `Bearer ${token}` },
    params: { page: 1, per_page: 50 },
  });
  expect(timeline.status()).toBe(200);
  const tl = (await timeline.json()) as { data?: TimelinePhoto[] };
  const found = (tl.data || []).find((p) => p.file_name === fileName) || tl.data?.[0];
  expect(found?.id).toBeTruthy();
  return {
    id: found.id as number,
    file_name: found.file_name,
  };
}

test.describe('Upload and Trash', () => {
  test('uploaded photo appears in timeline API and timeline page is reachable', async ({
    page,
    request,
  }) => {
    const user = await registerUser(request, 'e2e_upload_ui');
    await createLibrary(request, user.token, `upload-ui-${Date.now()}`);
    const fileName = `ui-visible-${Date.now()}.png`;

    const uploadRes = await request.post(`${API}/photos/upload`, {
      headers: { Authorization: `Bearer ${user.token}` },
      multipart: {
        file: {
          name: fileName,
          mimeType: 'image/png',
          buffer: SAMPLE_PNG,
        },
      },
    });
    expect(uploadRes.ok()).toBeTruthy();

    const timelineRes = await request.get(`${API}/photos/timeline`, {
      headers: { Authorization: `Bearer ${user.token}` },
    });
    expect(timelineRes.status()).toBe(200);
    const timeline = await timelineRes.json();
    expect(timeline.total).toBeGreaterThan(0);

    await page.goto('/login');
    await page.getByLabel('用户名').fill(user.username);
    await page.getByLabel('密码').fill(user.password);
    await page.getByRole('button', { name: /登录|创建管理员/ }).click();
    await expect(page).toHaveURL(/\/$/);
    await expect(page.locator('h1', { hasText: '照片' })).toBeVisible();
  });

  test('trash page shows deleted photo and can restore', async ({ page, request }) => {
    const user = await registerUser(request, 'e2e_trash');
    await createLibrary(request, user.token, `trash-${Date.now()}`);
    const uploaded = await uploadViaApi(request, user.token, `trash-${Date.now()}.png`);

    const trashRes = await request.post(`${API}/photos/${uploaded.id}/trash`, {
      headers: { Authorization: `Bearer ${user.token}` },
    });
    expect(trashRes.status()).toBe(200);

    await page.goto('/login');
    await page.getByLabel('用户名').fill(user.username);
    await page.getByLabel('密码').fill(user.password);
    await page.getByRole('button', { name: /登录|创建管理员/ }).click();
    await expect(page).toHaveURL(/\/$/);

    await page.getByRole('button', { name: '回收站' }).click();
    await expect(page).toHaveURL(/\/trash$/);
    await expect(page.locator('.photo-item')).toHaveCount(1, { timeout: 10000 });

    await page.locator('.trash-actions button[title="还原"]').first().click();
    await expect(page.getByText('回收站是空的')).toBeVisible({ timeout: 10000 });
  });

  test('duplicate upload renames file and empty trash reclaims original name', async ({ request }) => {
    const user = await registerUser(request, 'e2e_upload_collision');
    await createLibrary(request, user.token, `upload-collision-${Date.now()}`);

    const dupBaseName = `dup-${Date.now()}.png`;
    const dupFirst = await uploadViaApi(request, user.token, dupBaseName);
    const dupSecond = await uploadViaApi(request, user.token, dupBaseName);
    expect(dupFirst.file_name).toBe(dupBaseName);
    expect(dupSecond.file_name).toMatch(/^dup-\d+ \(1\)\.png$/);

    const reclaimName = `reclaim-${Date.now()}.png`;
    const reclaimFirst = await uploadViaApi(request, user.token, reclaimName);
    expect(reclaimFirst.file_name).toBe(reclaimName);

    const trashRes = await request.post(`${API}/photos/${reclaimFirst.id}/trash`, {
      headers: { Authorization: `Bearer ${user.token}` },
    });
    expect(trashRes.status()).toBe(200);

    const emptyRes = await request.post(`${API}/photos/trash/empty`, {
      headers: { Authorization: `Bearer ${user.token}` },
    });
    expect(emptyRes.status()).toBe(200);
    const emptyBody = await emptyRes.json();
    expect(emptyBody.deleted).toBe(1);

    const reclaimSecond = await uploadViaApi(request, user.token, reclaimName);
    expect(reclaimSecond.file_name).toBe(reclaimName);
  });
});
