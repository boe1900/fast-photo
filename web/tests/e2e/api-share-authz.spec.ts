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
  if (body?.uploaded?.[0]?.id) {
    return body.uploaded[0].id as number;
  }

  const timeline = await request.get(`${API}/photos/timeline`, {
    headers: { Authorization: `Bearer ${token}` },
    params: { page: 1, per_page: 50 },
  });
  expect(timeline.status()).toBe(200);
  const tl = (await timeline.json()) as { data?: TimelinePhoto[] };
  const found = (tl.data || []).find((p) => p.file_name === fileName) || tl.data?.[0];
  expect(found?.id).toBeTruthy();
  return found.id as number;
}

test.describe('API Share and AuthZ', () => {
  test('share password flow works on public endpoints', async ({ request }) => {
    const user = await registerUser(request, 'e2e_share');
    await createLibrary(request, user.token, `share-${Date.now()}`);
    const photoId = await uploadViaApi(request, user.token, `share-${Date.now()}.png`);

    const createAlbumRes = await request.post(`${API}/albums`, {
      headers: { Authorization: `Bearer ${user.token}` },
      data: { name: `share-album-${Date.now()}` },
    });
    expect(createAlbumRes.status()).toBe(200);
    const createBody = await createAlbumRes.json();
    let albumId = createBody.id as number | undefined;
    let shareToken = createBody.share_token as string | undefined;
    if (!albumId || !shareToken) {
      const listRes = await request.get(`${API}/albums`, {
        headers: { Authorization: `Bearer ${user.token}` },
      });
      expect(listRes.status()).toBe(200);
      const albums = (await listRes.json()) as Array<{ id?: number; share_token?: string }>;
      const latest = albums[0];
      albumId = latest?.id;
      shareToken = latest?.share_token;
    }
    expect(albumId).toBeTruthy();
    expect(shareToken).toBeTruthy();

    const addRes = await request.post(`${API}/albums/${albumId}/photos`, {
      headers: { Authorization: `Bearer ${user.token}` },
      data: { photo_ids: [photoId] },
    });
    expect(addRes.status()).toBe(200);

    const setPwRes = await request.post(`${API}/albums/${albumId}/share-password`, {
      headers: { Authorization: `Bearer ${user.token}` },
      data: { password: 'secret123' },
    });
    expect(setPwRes.status()).toBe(200);

    const infoRes = await request.get(`${API}/share/${shareToken}`);
    expect(infoRes.status()).toBe(200);
    const info = await infoRes.json();
    expect(info.has_password).toBe(true);

    const noPwPhotos = await request.get(`${API}/share/${shareToken}/photos`);
    expect(noPwPhotos.status()).toBe(401);

    const wrongVerify = await request.post(`${API}/share/${shareToken}/verify`, {
      data: { password: 'bad' },
    });
    expect(wrongVerify.status()).toBe(401);

    const verifyRes = await request.post(`${API}/share/${shareToken}/verify`, {
      data: { password: 'secret123' },
    });
    expect(verifyRes.status()).toBe(200);
    const verify = await verifyRes.json();
    expect(verify.valid).toBe(true);

    const withPwPhotos = await request.get(`${API}/share/${shareToken}/photos`, {
      headers: { 'x-share-password': 'secret123' },
    });
    expect(withPwPhotos.status()).toBe(200);
    const photos = await withPwPhotos.json();
    expect(photos.total).toBe(1);
  });

  test('cross-user photo and album access is forbidden', async ({ request }) => {
    const userA = await registerUser(request, 'e2e_authz_a');
    const userB = await registerUser(request, 'e2e_authz_b');

    await createLibrary(request, userA.token, `authz-a-${Date.now()}`);
    await createLibrary(request, userB.token, `authz-b-${Date.now()}`);

    const aPhotoId = await uploadViaApi(request, userA.token, `a-${Date.now()}.png`);
    const bPhotoId = await uploadViaApi(request, userB.token, `b-${Date.now()}.png`);

    const bAlbumRes = await request.post(`${API}/albums`, {
      headers: { Authorization: `Bearer ${userB.token}` },
      data: { name: `b-album-${Date.now()}` },
    });
    expect(bAlbumRes.status()).toBe(200);
    const bAlbumBody = await bAlbumRes.json();
    let bAlbumId = bAlbumBody.id as number | undefined;
    if (!bAlbumId) {
      const listRes = await request.get(`${API}/albums`, {
        headers: { Authorization: `Bearer ${userB.token}` },
      });
      expect(listRes.status()).toBe(200);
      const albums = (await listRes.json()) as Array<{ id?: number }>;
      bAlbumId = albums[0]?.id;
    }
    expect(bAlbumId).toBeTruthy();

    const bAlbumAdd = await request.post(`${API}/albums/${bAlbumId}/photos`, {
      headers: { Authorization: `Bearer ${userB.token}` },
      data: { photo_ids: [bPhotoId] },
    });
    expect(bAlbumAdd.status()).toBe(200);

    const forbiddenDetail = await request.get(`${API}/photos/${bPhotoId}`, {
      headers: { Authorization: `Bearer ${userA.token}` },
    });
    expect(forbiddenDetail.status()).toBe(403);

    const forbiddenFavorite = await request.post(`${API}/photos/${bPhotoId}/favorite`, {
      headers: { Authorization: `Bearer ${userA.token}` },
    });
    expect(forbiddenFavorite.status()).toBe(403);

    const forbiddenAlbumGet = await request.get(`${API}/albums/${bAlbumId}`, {
      headers: { Authorization: `Bearer ${userA.token}` },
    });
    expect(forbiddenAlbumGet.status()).toBe(403);

    const forbiddenAlbumAdd = await request.post(`${API}/albums/${bAlbumId}/photos`, {
      headers: { Authorization: `Bearer ${userA.token}` },
      data: { photo_ids: [aPhotoId] },
    });
    expect(forbiddenAlbumAdd.status()).toBe(403);
  });
});
