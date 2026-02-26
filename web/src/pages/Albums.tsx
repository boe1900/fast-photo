import { useState, useEffect } from 'react';
import { photoApi } from '../api';
import PhotoViewer from '../components/PhotoViewer';
import { BookImage, Plus, Trash2, Share2, X, Check } from 'lucide-react';
import axios from 'axios';

interface Album {
    id: number;
    name: string;
    photo_count: number;
    share_token: string | null;
    cover_photo_id: number | null;
    created_at: string;
}

interface Photo {
    id: number;
    file_name: string;
    taken_at: string | null;
    created_at: string;
    width: number | null;
    height: number | null;
    mime_type: string;
}

export default function Albums() {
    const [albums, setAlbums] = useState<Album[]>([]);
    const [selectedAlbum, setSelectedAlbum] = useState<Album | null>(null);
    const [albumPhotos, setAlbumPhotos] = useState<Photo[]>([]);
    const [showCreate, setShowCreate] = useState(false);
    const [newName, setNewName] = useState('');
    const [viewerPhoto, setViewerPhoto] = useState<Photo | null>(null);
    const [copied, setCopied] = useState(false);

    const token = localStorage.getItem('token');
    const headers = { Authorization: `Bearer ${token}` };

    useEffect(() => {
        loadAlbums();
    }, []);

    const loadAlbums = async () => {
        try {
            const res = await axios.get('/api/albums', { headers });
            setAlbums(res.data);
        } catch (err) {
            console.error('Failed to load albums:', err);
        }
    };

    const createAlbum = async () => {
        if (!newName.trim()) return;
        try {
            await axios.post('/api/albums', { name: newName }, { headers });
            setNewName('');
            setShowCreate(false);
            loadAlbums();
        } catch (err) {
            console.error('Failed to create album:', err);
        }
    };

    const deleteAlbum = async (id: number) => {
        if (!confirm('确定删除这个相册？')) return;
        try {
            await axios.delete(`/api/albums/${id}`, { headers });
            if (selectedAlbum?.id === id) {
                setSelectedAlbum(null);
                setAlbumPhotos([]);
            }
            loadAlbums();
        } catch (err) {
            console.error('Failed to delete album:', err);
        }
    };

    const selectAlbum = async (album: Album) => {
        setSelectedAlbum(album);
        try {
            const res = await axios.get(`/api/albums/${album.id}/photos`, { headers, params: { per_page: 200 } });
            setAlbumPhotos(res.data.data);
        } catch (err) {
            console.error('Failed to load album photos:', err);
        }
    };

    const copyShareLink = (album: Album) => {
        if (album.share_token) {
            const link = `${window.location.origin}/share/${album.share_token}`;
            navigator.clipboard.writeText(link);
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
        }
    };

    return (
        <>
            <div className="content-header">
                <h1>
                    <BookImage size={20} style={{ display: 'inline', marginRight: '8px', verticalAlign: 'middle', color: 'var(--accent)' }} />
                    相册
                </h1>
                <div className="header-actions">
                    <button className="btn btn-primary" onClick={() => setShowCreate(true)}>
                        <Plus size={14} />
                        创建相册
                    </button>
                </div>
            </div>

            <div className="content-body">
                {/* Create album modal */}
                {showCreate && (
                    <div className="modal-overlay" onClick={() => setShowCreate(false)}>
                        <div className="modal" onClick={(e) => e.stopPropagation()}>
                            <div className="modal-header">
                                <h3>创建相册</h3>
                                <button className="btn-icon" onClick={() => setShowCreate(false)}><X size={18} /></button>
                            </div>
                            <div className="modal-body">
                                <label>相册名称</label>
                                <input
                                    type="text"
                                    value={newName}
                                    onChange={(e) => setNewName(e.target.value)}
                                    placeholder="输入相册名称"
                                    autoFocus
                                    onKeyDown={(e) => e.key === 'Enter' && createAlbum()}
                                />
                            </div>
                            <div className="modal-footer">
                                <button className="btn btn-secondary" onClick={() => setShowCreate(false)}>取消</button>
                                <button className="btn btn-primary" onClick={createAlbum}>创建</button>
                            </div>
                        </div>
                    </div>
                )}

                {/* Album grid */}
                {!selectedAlbum && (
                    <>
                        {albums.length === 0 ? (
                            <div className="empty-state">
                                <BookImage size={48} />
                                <h3>还没有相册</h3>
                                <p>点击"创建相册"来整理你的照片</p>
                            </div>
                        ) : (
                            <div className="album-grid">
                                {albums.map((album) => (
                                    <div key={album.id} className="album-card" onClick={() => selectAlbum(album)}>
                                        <div className="album-cover">
                                            {album.cover_photo_id ? (
                                                <img src={photoApi.thumbnailUrl(album.cover_photo_id, 'medium')} alt={album.name} />
                                            ) : (
                                                <div className="album-cover-empty"><BookImage size={32} /></div>
                                            )}
                                        </div>
                                        <div className="album-info">
                                            <div className="album-name">{album.name}</div>
                                            <div className="album-count">{album.photo_count} 张照片</div>
                                        </div>
                                        <div className="album-actions" onClick={(e) => e.stopPropagation()}>
                                            <button className="btn-icon" onClick={() => copyShareLink(album)} title="复制分享链接">
                                                {copied ? <Check size={14} /> : <Share2 size={14} />}
                                            </button>
                                            <button className="btn-icon" onClick={() => deleteAlbum(album.id)} title="删除相册">
                                                <Trash2 size={14} />
                                            </button>
                                        </div>
                                    </div>
                                ))}
                            </div>
                        )}
                    </>
                )}

                {/* Album detail view */}
                {selectedAlbum && (
                    <>
                        <div style={{ marginBottom: '16px', display: 'flex', alignItems: 'center', gap: '12px' }}>
                            <button className="btn btn-secondary" onClick={() => { setSelectedAlbum(null); setAlbumPhotos([]); }}>
                                ← 返回
                            </button>
                            <h2 style={{ fontSize: '16px', fontWeight: 600 }}>{selectedAlbum.name}</h2>
                            <span style={{ color: 'var(--text-muted)', fontSize: '13px' }}>{albumPhotos.length} 张照片</span>
                        </div>

                        {albumPhotos.length === 0 ? (
                            <div className="empty-state">
                                <BookImage size={48} />
                                <h3>相册为空</h3>
                                <p>在照片时间线中选择照片添加到相册</p>
                            </div>
                        ) : (
                            <div className="photo-grid">
                                {albumPhotos.map((photo) => (
                                    <div key={photo.id} className="photo-item" onClick={() => setViewerPhoto(photo)}>
                                        <img src={photoApi.thumbnailUrl(photo.id, 'small')} alt={photo.file_name} loading="lazy" />
                                    </div>
                                ))}
                            </div>
                        )}
                    </>
                )}
            </div>

            {viewerPhoto && (
                <PhotoViewer photo={viewerPhoto} onClose={() => setViewerPhoto(null)} />
            )}
        </>
    );
}
