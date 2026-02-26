import { useState, useEffect } from 'react';
import { photoApi } from '../api';
import PhotoViewer from '../components/PhotoViewer';
import { Heart, HeartOff } from 'lucide-react';

interface Photo {
    id: number;
    file_name: string;
    taken_at: string | null;
    created_at: string;
    width: number | null;
    height: number | null;
    is_favorite: boolean;
    mime_type: string;
}

export default function Favorites() {
    const [photos, setPhotos] = useState<Photo[]>([]);
    const [total, setTotal] = useState(0);
    const [loading, setLoading] = useState(true);
    const [viewerPhoto, setViewerPhoto] = useState<Photo | null>(null);

    const load = async () => {
        setLoading(true);
        try {
            const res = await photoApi.favorites();
            setPhotos(res.data.data);
            setTotal(res.data.total);
        } catch (err) {
            console.error('Failed to load favorites:', err);
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => { load(); }, []);

    const handleUnfavorite = async (id: number, e: React.MouseEvent) => {
        e.stopPropagation();
        await photoApi.toggleFavorite(id);
        setPhotos(prev => prev.filter(p => p.id !== id));
        setTotal(prev => prev - 1);
    };

    return (
        <>
            <div className="content-header">
                <h1>收藏</h1>
                <span style={{ color: 'var(--text-muted)', fontSize: '13px' }}>{total} 张照片</span>
            </div>
            <div className="content-body">
                {loading ? (
                    <div className="loading-spinner"><div className="spinner" /></div>
                ) : photos.length === 0 ? (
                    <div className="empty-state">
                        <Heart size={48} />
                        <h2>还没有收藏</h2>
                        <p>在照片详情中点击 ♥ 添加收藏</p>
                    </div>
                ) : (
                    <div className="photo-grid">
                        {photos.map(photo => (
                            <div key={photo.id} className="photo-item" onClick={() => setViewerPhoto(photo)}>
                                <img src={photoApi.thumbnailUrl(photo.id, 'small')} alt={photo.file_name} loading="lazy" />
                                <button className="unfav-btn" onClick={(e) => handleUnfavorite(photo.id, e)}>
                                    <HeartOff size={14} />
                                </button>
                            </div>
                        ))}
                    </div>
                )}
            </div>
            {viewerPhoto && <PhotoViewer photo={viewerPhoto} onClose={() => setViewerPhoto(null)} />}
        </>
    );
}
