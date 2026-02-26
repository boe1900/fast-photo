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

async function timelinePhotoIds(request: APIRequestContext, token: string): Promise<number[]> {
  const res = await request.get(`${API}/photos/timeline`, {
    headers: { Authorization: `Bearer ${token}` },
    params: { page: 1, per_page: 100 },
  });
  expect(res.status()).toBe(200);
  const body = await res.json();
  return (body.data || []).map((p: { id: number }) => p.id);
}

async function trashPhotoIds(request: APIRequestContext, token: string): Promise<number[]> {
  const res = await request.get(`${API}/photos/trash`, {
    headers: { Authorization: `Bearer ${token}` },
    params: { page: 1, per_page: 100 },
  });
  expect(res.status()).toBe(200);
  const body = await res.json();
  return (body.data || []).map((p: { id: number }) => p.id);
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

  test('share password + trash + restore keep authz boundaries', async ({ request }) => {
    const owner = await registerUser(request, 'e2e_combo_owner');
    const outsider = await registerUser(request, 'e2e_combo_outsider');

    await createLibrary(request, owner.token, `combo-owner-${Date.now()}`);
    await createLibrary(request, outsider.token, `combo-outsider-${Date.now()}`);
    const ownerPhotoId = await uploadViaApi(request, owner.token, `combo-${Date.now()}.png`);

    const createAlbumRes = await request.post(`${API}/albums`, {
      headers: { Authorization: `Bearer ${owner.token}` },
      data: { name: `combo-album-${Date.now()}` },
    });
    expect(createAlbumRes.status()).toBe(200);
    const createBody = await createAlbumRes.json();
    let albumId = createBody.id as number | undefined;
    let shareToken = createBody.share_token as string | undefined;
    if (!albumId || !shareToken) {
      const listRes = await request.get(`${API}/albums`, {
        headers: { Authorization: `Bearer ${owner.token}` },
      });
      expect(listRes.status()).toBe(200);
      const albums = (await listRes.json()) as Array<{ id?: number; share_token?: string }>;
      albumId = albums[0]?.id;
      shareToken = albums[0]?.share_token;
    }
    expect(albumId).toBeTruthy();
    expect(shareToken).toBeTruthy();

    const addPhotoRes = await request.post(`${API}/albums/${albumId}/photos`, {
      headers: { Authorization: `Bearer ${owner.token}` },
      data: { photo_ids: [ownerPhotoId] },
    });
    expect(addPhotoRes.status()).toBe(200);

    const setPwByOwner = await request.post(`${API}/albums/${albumId}/share-password`, {
      headers: { Authorization: `Bearer ${owner.token}` },
      data: { password: 'combo-secret' },
    });
    expect(setPwByOwner.status()).toBe(200);

    const setPwByOutsider = await request.post(`${API}/albums/${albumId}/share-password`, {
      headers: { Authorization: `Bearer ${outsider.token}` },
      data: { password: 'hijack' },
    });
    expect(setPwByOutsider.status()).toBe(403);

    const noPwShared = await request.get(`${API}/share/${shareToken}/photos`);
    expect(noPwShared.status()).toBe(401);

    const sharedWithPw = await request.get(`${API}/share/${shareToken}/photos`, {
      headers: { 'x-share-password': 'combo-secret' },
    });
    expect(sharedWithPw.status()).toBe(200);
    const sharedBeforeTrash = await sharedWithPw.json();
    expect(sharedBeforeTrash.total).toBe(1);

    const outsiderTrash = await request.post(`${API}/photos/${ownerPhotoId}/trash`, {
      headers: { Authorization: `Bearer ${outsider.token}` },
    });
    expect(outsiderTrash.status()).toBe(403);

    const ownerTrash = await request.post(`${API}/photos/${ownerPhotoId}/trash`, {
      headers: { Authorization: `Bearer ${owner.token}` },
    });
    expect(ownerTrash.status()).toBe(200);

    const ownerTrashList = await request.get(`${API}/photos/trash`, {
      headers: { Authorization: `Bearer ${owner.token}` },
    });
    expect(ownerTrashList.status()).toBe(200);
    const ownerTrashBody = await ownerTrashList.json();
    const trashedIds = (ownerTrashBody.data || []).map((p: { id: number }) => p.id);
    expect(trashedIds).toContain(ownerPhotoId);

    const outsiderRestore = await request.post(`${API}/photos/${ownerPhotoId}/restore`, {
      headers: { Authorization: `Bearer ${outsider.token}` },
    });
    expect(outsiderRestore.status()).toBe(403);

    const outsiderDetail = await request.get(`${API}/photos/${ownerPhotoId}`, {
      headers: { Authorization: `Bearer ${outsider.token}` },
    });
    expect(outsiderDetail.status()).toBe(403);

    const ownerRestore = await request.post(`${API}/photos/${ownerPhotoId}/restore`, {
      headers: { Authorization: `Bearer ${owner.token}` },
    });
    expect(ownerRestore.status()).toBe(200);

    const ownerTrashAfterRestore = await request.get(`${API}/photos/trash`, {
      headers: { Authorization: `Bearer ${owner.token}` },
    });
    expect(ownerTrashAfterRestore.status()).toBe(200);
    const ownerTrashAfterRestoreBody = await ownerTrashAfterRestore.json();
    const trashIdsAfterRestore = (ownerTrashAfterRestoreBody.data || []).map((p: { id: number }) => p.id);
    expect(trashIdsAfterRestore).not.toContain(ownerPhotoId);

    const sharedAfterRestore = await request.get(`${API}/share/${shareToken}/photos`, {
      headers: { 'x-share-password': 'combo-secret' },
    });
    expect(sharedAfterRestore.status()).toBe(200);
    const sharedAfterRestoreBody = await sharedAfterRestore.json();
    expect(sharedAfterRestoreBody.total).toBe(1);
  });

  test('batch operations only affect caller-owned photos', async ({ request }) => {
    const userA = await registerUser(request, 'e2e_batch_a');
    const userB = await registerUser(request, 'e2e_batch_b');

    await createLibrary(request, userA.token, `batch-a-${Date.now()}`);
    await createLibrary(request, userB.token, `batch-b-${Date.now()}`);

    const aPhotoId = await uploadViaApi(request, userA.token, `batch-a-${Date.now()}.png`);
    const bPhotoId = await uploadViaApi(request, userB.token, `batch-b-${Date.now()}.png`);

    const aBatchFavorite = await request.post(`${API}/photos/batch/favorite`, {
      headers: { Authorization: `Bearer ${userA.token}` },
      data: { photo_ids: [aPhotoId, bPhotoId], favorite: true },
    });
    expect(aBatchFavorite.status()).toBe(200);
    const aBatchFavoriteBody = await aBatchFavorite.json();
    expect(aBatchFavoriteBody.affected).toBe(1);

    const aFavorites = await request.get(`${API}/photos/favorites`, {
      headers: { Authorization: `Bearer ${userA.token}` },
    });
    expect(aFavorites.status()).toBe(200);
    const aFavoritesBody = await aFavorites.json();
    const aFavoriteIds = (aFavoritesBody.data || []).map((p: { id: number }) => p.id);
    expect(aFavoriteIds).toContain(aPhotoId);
    expect(aFavoriteIds).not.toContain(bPhotoId);

    const bFavorites = await request.get(`${API}/photos/favorites`, {
      headers: { Authorization: `Bearer ${userB.token}` },
    });
    expect(bFavorites.status()).toBe(200);
    const bFavoritesBody = await bFavorites.json();
    const bFavoriteIds = (bFavoritesBody.data || []).map((p: { id: number }) => p.id);
    expect(bFavoriteIds).not.toContain(bPhotoId);

    const bBatchTrashForeign = await request.post(`${API}/photos/batch/trash`, {
      headers: { Authorization: `Bearer ${userB.token}` },
      data: { photo_ids: [aPhotoId] },
    });
    expect(bBatchTrashForeign.status()).toBe(200);
    const bBatchTrashForeignBody = await bBatchTrashForeign.json();
    expect(bBatchTrashForeignBody.affected).toBe(0);

    const aBatchTrash = await request.post(`${API}/photos/batch/trash`, {
      headers: { Authorization: `Bearer ${userA.token}` },
      data: { photo_ids: [aPhotoId, bPhotoId] },
    });
    expect(aBatchTrash.status()).toBe(200);
    const aBatchTrashBody = await aBatchTrash.json();
    expect(aBatchTrashBody.affected).toBe(1);

    const aTrash = await request.get(`${API}/photos/trash`, {
      headers: { Authorization: `Bearer ${userA.token}` },
    });
    expect(aTrash.status()).toBe(200);
    const aTrashBody = await aTrash.json();
    const aTrashIds = (aTrashBody.data || []).map((p: { id: number }) => p.id);
    expect(aTrashIds).toContain(aPhotoId);
    expect(aTrashIds).not.toContain(bPhotoId);

    const bTrash = await request.get(`${API}/photos/trash`, {
      headers: { Authorization: `Bearer ${userB.token}` },
    });
    expect(bTrash.status()).toBe(200);
    const bTrashBody = await bTrash.json();
    const bTrashIds = (bTrashBody.data || []).map((p: { id: number }) => p.id);
    expect(bTrashIds).not.toContain(bPhotoId);

    const aBatchRestore = await request.post(`${API}/photos/batch/restore`, {
      headers: { Authorization: `Bearer ${userA.token}` },
      data: { photo_ids: [aPhotoId, bPhotoId] },
    });
    expect(aBatchRestore.status()).toBe(200);
    const aBatchRestoreBody = await aBatchRestore.json();
    expect(aBatchRestoreBody.affected).toBe(1);

    const aTrashAfterRestore = await request.get(`${API}/photos/trash`, {
      headers: { Authorization: `Bearer ${userA.token}` },
    });
    expect(aTrashAfterRestore.status()).toBe(200);
    const aTrashAfterRestoreBody = await aTrashAfterRestore.json();
    const aTrashIdsAfterRestore = (aTrashAfterRestoreBody.data || []).map((p: { id: number }) => p.id);
    expect(aTrashIdsAfterRestore).not.toContain(aPhotoId);

    const aBatchUnfavorite = await request.post(`${API}/photos/batch/favorite`, {
      headers: { Authorization: `Bearer ${userA.token}` },
      data: { photo_ids: [aPhotoId, bPhotoId], favorite: false },
    });
    expect(aBatchUnfavorite.status()).toBe(200);
    const aBatchUnfavoriteBody = await aBatchUnfavorite.json();
    expect(aBatchUnfavoriteBody.affected).toBe(1);
  });

  test('empty trash and permanent delete preserve authz and user data boundaries', async ({
    request,
  }) => {
    const userA = await registerUser(request, 'e2e_empty_perm_a');
    const userB = await registerUser(request, 'e2e_empty_perm_b');

    await createLibrary(request, userA.token, `empty-perm-a-${Date.now()}`);
    await createLibrary(request, userB.token, `empty-perm-b-${Date.now()}`);

    const aTrashedPhotoId = await uploadViaApi(request, userA.token, `a-trashed-${Date.now()}.png`);
    const aActivePhotoId = await uploadViaApi(request, userA.token, `a-active-${Date.now()}.png`);
    const bTrashedPhotoId = await uploadViaApi(request, userB.token, `b-trashed-${Date.now()}.png`);

    const trashA = await request.post(`${API}/photos/${aTrashedPhotoId}/trash`, {
      headers: { Authorization: `Bearer ${userA.token}` },
    });
    expect(trashA.status()).toBe(200);

    const trashB = await request.post(`${API}/photos/${bTrashedPhotoId}/trash`, {
      headers: { Authorization: `Bearer ${userB.token}` },
    });
    expect(trashB.status()).toBe(200);

    const beforeATrash = await trashPhotoIds(request, userA.token);
    const beforeBTrash = await trashPhotoIds(request, userB.token);
    expect(beforeATrash).toContain(aTrashedPhotoId);
    expect(beforeBTrash).toContain(bTrashedPhotoId);

    const emptyATrash = await request.post(`${API}/photos/trash/empty`, {
      headers: { Authorization: `Bearer ${userA.token}` },
    });
    expect(emptyATrash.status()).toBe(200);
    const emptyATrashBody = await emptyATrash.json();
    expect(emptyATrashBody.deleted).toBe(1);

    const afterATrash = await trashPhotoIds(request, userA.token);
    const afterBTrash = await trashPhotoIds(request, userB.token);
    expect(afterATrash).not.toContain(aTrashedPhotoId);
    expect(afterBTrash).toContain(bTrashedPhotoId);

    const bDeleteA = await request.delete(`${API}/photos/${aActivePhotoId}/permanent`, {
      headers: { Authorization: `Bearer ${userB.token}` },
    });
    expect(bDeleteA.status()).toBe(403);

    const aDeleteA = await request.delete(`${API}/photos/${aActivePhotoId}/permanent`, {
      headers: { Authorization: `Bearer ${userA.token}` },
    });
    expect(aDeleteA.status()).toBe(200);

    const aTimelineAfterDelete = await timelinePhotoIds(request, userA.token);
    const bTimelineAfterDelete = await timelinePhotoIds(request, userB.token);
    expect(aTimelineAfterDelete).not.toContain(aActivePhotoId);
    expect(bTimelineAfterDelete).not.toContain(aActivePhotoId);
    expect(bTimelineAfterDelete).not.toContain(aTrashedPhotoId);

    const repeatDelete = await request.delete(`${API}/photos/${aActivePhotoId}/permanent`, {
      headers: { Authorization: `Bearer ${userA.token}` },
    });
    expect([403, 404]).toContain(repeatDelete.status());
  });

  test('share token becomes invalid after album deletion', async ({ request }) => {
    const owner = await registerUser(request, 'e2e_share_delete_owner');
    const outsider = await registerUser(request, 'e2e_share_delete_outsider');

    await createLibrary(request, owner.token, `share-delete-${Date.now()}`);
    const photoId = await uploadViaApi(request, owner.token, `share-delete-${Date.now()}.png`);

    const createAlbumRes = await request.post(`${API}/albums`, {
      headers: { Authorization: `Bearer ${owner.token}` },
      data: { name: `share-delete-album-${Date.now()}` },
    });
    expect(createAlbumRes.status()).toBe(200);
    const createBody = await createAlbumRes.json();
    let albumId = createBody.id as number | undefined;
    let shareToken = createBody.share_token as string | undefined;
    if (!albumId || !shareToken) {
      const listRes = await request.get(`${API}/albums`, {
        headers: { Authorization: `Bearer ${owner.token}` },
      });
      expect(listRes.status()).toBe(200);
      const albums = (await listRes.json()) as Array<{ id?: number; share_token?: string }>;
      albumId = albums[0]?.id;
      shareToken = albums[0]?.share_token;
    }
    expect(albumId).toBeTruthy();
    expect(shareToken).toBeTruthy();

    const addRes = await request.post(`${API}/albums/${albumId}/photos`, {
      headers: { Authorization: `Bearer ${owner.token}` },
      data: { photo_ids: [photoId] },
    });
    expect(addRes.status()).toBe(200);

    const setPwRes = await request.post(`${API}/albums/${albumId}/share-password`, {
      headers: { Authorization: `Bearer ${owner.token}` },
      data: { password: 'to-delete-secret' },
    });
    expect(setPwRes.status()).toBe(200);

    const shareInfoBeforeDelete = await request.get(`${API}/share/${shareToken}`);
    expect(shareInfoBeforeDelete.status()).toBe(200);

    const sharePhotosBeforeDelete = await request.get(`${API}/share/${shareToken}/photos`, {
      headers: { 'x-share-password': 'to-delete-secret' },
    });
    expect(sharePhotosBeforeDelete.status()).toBe(200);
    const sharePhotosBeforeDeleteBody = await sharePhotosBeforeDelete.json();
    expect(sharePhotosBeforeDeleteBody.total).toBe(1);

    const outsiderDeleteAlbum = await request.delete(`${API}/albums/${albumId}`, {
      headers: { Authorization: `Bearer ${outsider.token}` },
    });
    expect(outsiderDeleteAlbum.status()).toBe(403);

    const ownerDeleteAlbum = await request.delete(`${API}/albums/${albumId}`, {
      headers: { Authorization: `Bearer ${owner.token}` },
    });
    expect(ownerDeleteAlbum.status()).toBe(204);

    const ownerGetDeletedAlbum = await request.get(`${API}/albums/${albumId}`, {
      headers: { Authorization: `Bearer ${owner.token}` },
    });
    expect(ownerGetDeletedAlbum.status()).toBe(404);

    const shareInfoAfterDelete = await request.get(`${API}/share/${shareToken}`);
    expect(shareInfoAfterDelete.status()).toBe(404);

    const sharePhotosAfterDelete = await request.get(`${API}/share/${shareToken}/photos`, {
      headers: { 'x-share-password': 'to-delete-secret' },
    });
    expect(sharePhotosAfterDelete.status()).toBe(404);

    const verifyAfterDelete = await request.post(`${API}/share/${shareToken}/verify`, {
      data: { password: 'to-delete-secret' },
    });
    expect(verifyAfterDelete.status()).toBe(404);
  });
});
