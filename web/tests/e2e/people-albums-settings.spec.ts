import { expect, test, type APIRequestContext, type Dialog, type Page } from '@playwright/test';
import { execSync } from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';

const API = 'http://127.0.0.1:8080/api';
const SAMPLE_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO6pFf0AAAAASUVORK5CYII=',
  'base64'
);
const ENABLE_STORAGE_UI_E2E = process.env.FASTPHOTO_ENABLE_STORAGE_UI_E2E === '1';
const STORAGE_COMPOSE_PROJECT = 'fast-photo-storage-ui-e2e';
const STORAGE_COMPOSE_FILE = path.resolve(process.cwd(), '..', 'docker-compose.storage-backends.yml');
const STORAGE_BUCKET = 'fast-photo-ui-test';

function runShell(cmd: string): void {
  execSync(cmd, { stdio: 'ignore' });
}

async function waitForHttp(url: string, tries = 120): Promise<void> {
  let lastError = '';
  for (let i = 0; i < tries; i += 1) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
      lastError = `status=${res.status}`;
    } catch (err) {
      lastError = String(err);
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error(`Timeout waiting for ${url}: ${lastError}`);
}

async function waitForWebDav(url: string, tries = 120): Promise<void> {
  let lastError = '';
  for (let i = 0; i < tries; i += 1) {
    try {
      const res = await fetch(url);
      if (res.status > 0 && res.status < 500) return;
      lastError = `status=${res.status}`;
    } catch (err) {
      lastError = String(err);
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error(`Timeout waiting for WebDAV ${url}: ${lastError}`);
}

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
    await page.getByLabel('本地存储路径').fill('/tmp');
    await page.getByRole('button', { name: '测试本地连接' }).click();
    await expect(page.getByText('本地存储连接成功')).toBeVisible({ timeout: 10000 });
    await page.getByRole('button', { name: '保存本地配置' }).click();
    await expect(page.getByText('存储配置已保存')).toBeVisible({ timeout: 10000 });

    await page.getByRole('button', { name: '活动日志' }).click();
    await expect(page.getByRole('heading', { name: '活动日志' })).toBeVisible();
    await expect(page.getByText('添加标签')).toBeVisible({ timeout: 10000 });
  });

  test.describe('storage remote backends (ui)', () => {
    test.setTimeout(180000);
    test.skip(!ENABLE_STORAGE_UI_E2E, 'Set FASTPHOTO_ENABLE_STORAGE_UI_E2E=1 to run remote storage UI test');

    test.beforeAll(async ({}, testInfo) => {
      testInfo.setTimeout(180000);
      runShell(`docker compose -p ${STORAGE_COMPOSE_PROJECT} -f "${STORAGE_COMPOSE_FILE}" up -d`);
      await waitForHttp('http://127.0.0.1:19000/minio/health/ready');
      await waitForWebDav('http://127.0.0.1:19080/');
      runShell(
        `docker run --rm --network ${STORAGE_COMPOSE_PROJECT}_default -e "MC_HOST_local=http://minioadmin:minioadmin123@minio:9000" minio/mc mb -p local/${STORAGE_BUCKET}`
      );
    });

    test.afterAll(async () => {
      runShell(`docker compose -p ${STORAGE_COMPOSE_PROJECT} -f "${STORAGE_COMPOSE_FILE}" down -v`);
    });

    test('settings page can test and save S3/WebDAV configs', async ({ page, request }) => {
      const user = await registerUser(request, 'e2e_storage_ui');
      await loginByUi(page, user.username, user.password);

      await page.getByRole('button', { name: '设置' }).click();
      await expect(page).toHaveURL(/\/settings$/);
      await page.getByRole('button', { name: '存储配置' }).click();
      await expect(page.getByRole('heading', { name: '存储配置' })).toBeVisible();

      const s3Card = page.locator('.storage-card').filter({
        has: page.getByRole('heading', { name: 'S3 兼容存储' }),
      });
      await s3Card.getByPlaceholder('https://s3.amazonaws.com').fill('http://127.0.0.1:19000');
      await s3Card.getByPlaceholder('us-east-1').fill('us-east-1');
      await s3Card.getByPlaceholder('my-photos-bucket').fill(STORAGE_BUCKET);
      await s3Card.getByPlaceholder('AKIAIOSFODNN7').fill('minioadmin');
      await s3Card.getByPlaceholder('••••••••').first().fill('wrong-secret');
      await s3Card.getByRole('button', { name: '测试 S3 连接' }).click();
      await expect(page.getByText('连接失败')).toBeVisible({ timeout: 15000 });

      await s3Card.getByPlaceholder('••••••••').first().fill('minioadmin123');
      await s3Card.getByRole('button', { name: '测试 S3 连接' }).click();
      await expect(page.getByText('连接成功')).toBeVisible({ timeout: 15000 });
      await s3Card.getByRole('button', { name: '保存 S3 配置' }).click();
      await expect(page.getByText('存储配置已保存')).toBeVisible({ timeout: 15000 });

      const webDavCard = page.locator('.storage-card').filter({
        has: page.getByRole('heading', { name: 'WebDAV' }),
      });
      await webDavCard.getByPlaceholder('https://example.com/webdav').fill('http://127.0.0.1:19080');
      await webDavCard.getByPlaceholder('admin').fill('webdav_user');
      await webDavCard.getByPlaceholder('••••••••').first().fill('wrong-pass');
      await webDavCard.getByRole('button', { name: '测试 WebDAV 连接' }).click();
      await expect(page.getByText('连接失败')).toBeVisible({ timeout: 15000 });

      await webDavCard.getByPlaceholder('••••••••').first().fill('webdav_pass');
      await webDavCard.getByRole('button', { name: '测试 WebDAV 连接' }).click();
      await expect(page.getByText('连接成功')).toBeVisible({ timeout: 15000 });
      await webDavCard.getByRole('button', { name: '保存 WebDAV 配置' }).click();
      await expect(page.getByText('存储配置已保存')).toBeVisible({ timeout: 15000 });
      await expect(
        webDavCard.getByText('当前使用'),
      ).toBeVisible({ timeout: 15000 });
    });
  });
});
