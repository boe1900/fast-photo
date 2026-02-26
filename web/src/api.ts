import axios from 'axios';

const api = axios.create({
    baseURL: '/api',
});

// Attach JWT to all requests
api.interceptors.request.use((config) => {
    const token = localStorage.getItem('token');
    if (token) {
        config.headers.Authorization = `Bearer ${token}`;
    }
    return config;
});

// Handle 401 — redirect to login
api.interceptors.response.use(
    (res) => res,
    (err) => {
        if (err.response?.status === 401) {
            localStorage.removeItem('token');
            localStorage.removeItem('user');
            if (window.location.pathname !== '/login') {
                window.location.href = '/login';
            }
        }
        return Promise.reject(err);
    }
);

// ─── Auth ──────────────────────────────────────

export const authApi = {
    setupStatus: () => api.get('/auth/setup-status'),
    login: (username: string, password: string) =>
        api.post('/auth/login', { username, password }),
    register: (username: string, password: string) =>
        api.post('/auth/register', { username, password }),
    me: () => api.get('/auth/me'),
};

// ─── Libraries ─────────────────────────────────

export const libraryApi = {
    list: () => api.get('/libraries'),
    create: (name: string, path: string) =>
        api.post('/libraries', { name, path }),
    delete: (id: number) => api.delete(`/libraries/${id}`),
    scan: (id: number) => api.post(`/libraries/${id}/scan`),
    scanProgress: () => api.get('/libraries/scan-progress'),
};

// ─── Photos ────────────────────────────────────

export const photoApi = {
    timeline: (page = 1, perPage = 50) =>
        api.get('/photos/timeline', { params: { page, per_page: perPage } }),
    folders: () => api.get('/photos/folders'),
    folderContents: (path: string, page = 1, perPage = 50) =>
        api.get('/photos/folder-contents', { params: { path, page, per_page: perPage } }),
    search: (q: string, page = 1, perPage = 50) =>
        api.get('/photos/search', { params: { q, page, per_page: perPage } }),
    detail: (id: number) => api.get(`/photos/${id}`),
    thumbnailUrl: (id: number, size: 'small' | 'medium' | 'large' = 'small') =>
        `/api/photos/${id}/thumbnail/${size}`,
    originalUrl: (id: number) => `/api/photos/${id}/original`,
    // Favorites
    toggleFavorite: (id: number) => api.post(`/photos/${id}/favorite`),
    favorites: (page = 1, perPage = 50) =>
        api.get('/photos/favorites', { params: { page, per_page: perPage } }),
    batchFavorite: (photoIds: number[], favorite: boolean) =>
        api.post('/photos/batch/favorite', { photo_ids: photoIds, favorite }),
    // Trash
    trashPhoto: (id: number) => api.post(`/photos/${id}/trash`),
    restorePhoto: (id: number) => api.post(`/photos/${id}/restore`),
    permanentDelete: (id: number) => api.delete(`/photos/${id}/permanent`),
    trashList: (page = 1, perPage = 50) =>
        api.get('/photos/trash', { params: { page, per_page: perPage } }),
    emptyTrash: () => api.post('/photos/trash/empty'),
    batchTrash: (photoIds: number[]) =>
        api.post('/photos/batch/trash', { photo_ids: photoIds }),
    batchRestore: (photoIds: number[]) =>
        api.post('/photos/batch/restore', { photo_ids: photoIds }),
    // Upload
    upload: (files: FormData) =>
        api.post('/photos/upload', files, { headers: { 'Content-Type': 'multipart/form-data' } }),
    // Duplicates
    duplicates: () => api.get('/photos/duplicates'),
};

// ─── Faces / Persons ───────────────────────────

export const faceApi = {
    listPersons: () => api.get('/persons'),
    renamePerson: (id: number, name: string) => api.put(`/persons/${id}`, { name }),
    personPhotos: (id: number) => api.get(`/persons/${id}/photos`),
    scanFaces: () => api.post('/faces/scan'),
    clusterFaces: () => api.post('/faces/cluster'),
    faceThumbnailUrl: (id: number) => `/api/faces/${id}/thumbnail`,
};

// ─── Tags ───────────────────────────────────────

export const tagApi = {
    listAll: () => api.get('/tags'),
    photoTags: (photoId: number) => api.get(`/photos/${photoId}/tags`),
    addTag: (photoId: number, name: string, category = 'manual') =>
        api.post(`/photos/${photoId}/tags`, { name, category }),
    removeTag: (photoId: number, tagId: number) =>
        api.delete(`/photos/${photoId}/tags/${tagId}`),
    searchByTag: (tag: string, page = 1, perPage = 50) =>
        api.get('/tags/photos', { params: { tag, page, per_page: perPage } }),
};

// ─── Activity Log ───────────────────────────────

export const activityApi = {
    list: () => api.get('/activity'),
};

// ─── Albums ─────────────────────────────────────

export const albumApi = {
    list: () => api.get('/albums'),
    create: (name: string) => api.post('/albums', { name }),
    get: (id: number) => api.get(`/albums/${id}`),
    delete: (id: number) => api.delete(`/albums/${id}`),
    photos: (id: number) => api.get(`/albums/${id}/photos`),
    addPhotos: (id: number, photoIds: number[]) =>
        api.post(`/albums/${id}/photos`, { photo_ids: photoIds }),
    removePhoto: (id: number, photoId: number) =>
        api.delete(`/albums/${id}/photos/${photoId}`),
    setSharePassword: (id: number, password: string | null) =>
        api.post(`/albums/${id}/share-password`, { password }),
    // Public share
    sharedAlbum: (token: string) => api.get(`/share/${token}`),
    verifyPassword: (token: string, password: string) =>
        api.post(`/share/${token}/verify`, { password }),
    sharedPhotos: (token: string, password?: string) =>
        api.get(`/share/${token}/photos`, {
            headers: password ? { 'x-share-password': password } : {},
        }),
};

export default api;
